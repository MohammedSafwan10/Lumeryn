#!/usr/bin/env python3
"""Verify, audit or reproducibly acquire the reviewed offline asset catalog.

The default is offline verification. Acquire only installs into a fresh directory;
it never removes or overwrites an installed library, project or source media.
"""
import argparse
import concurrent.futures
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import struct
import subprocess
import tempfile
import urllib.parse
import urllib.request
import zipfile

QUOTA = 64 * 1024 * 1024
DOWNLOAD_LIMIT = 32 * 1024 * 1024
HOSTS = {"kenney.nl", "opengameart.org", "github.com", "objects.githubusercontent.com",
         "release-assets.githubusercontent.com", "raw.githubusercontent.com", "creativecommons.org"}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def confined(root, name):
    value = PurePosixPath(name)
    if not name or "\\" in name or value.is_absolute() or any(p in {"..", "."} or ":" in p for p in value.parts):
        raise ValueError(f"unsafe relative path: {name}")
    result = root.joinpath(*value.parts)
    if not result.resolve().is_relative_to(root.resolve()):
        raise ValueError("path escapes library")
    return result


def download(url, sha256):
    class Redirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, request, fp, code, msg, headers, target):
            validate_url(target)
            return super().redirect_request(request, fp, code, msg, headers, target)

    validate_url(url)
    request = urllib.request.Request(url, headers={"User-Agent": "LumerynAssetAudit/1"})
    with urllib.request.build_opener(Redirect()).open(request, timeout=30) as response:
        advertised = response.headers.get("Content-Length")
        if advertised and int(advertised) > DOWNLOAD_LIMIT:
            raise ValueError("advertised download exceeds quota")
        data = response.read(DOWNLOAD_LIMIT + 1)
    if len(data) > DOWNLOAD_LIMIT or digest(data) != sha256:
        raise ValueError(f"download quota or pinned SHA256 mismatch: {url}")
    return data


def validate_url(url):
    parsed = urllib.parse.urlsplit(url)
    if parsed.scheme != "https" or parsed.hostname not in HOSTS or parsed.username or parsed.password:
        raise ValueError("unreviewed HTTPS host or credential-bearing URL")


def safe_archive(data):
    archive = zipfile.ZipFile(io.BytesIO(data))
    infos = archive.infolist()
    if len(infos) > 2048 or sum(i.file_size for i in infos) > QUOTA:
        raise ValueError("archive exceeds unpacked quota")
    seen = set()
    for info in infos:
        name = info.filename.rstrip("/")
        confined(Path.cwd(), name)
        if name.casefold() in seen or (info.external_attr >> 16) & 0o170000 == 0o120000:
            raise ValueError("duplicate archive path or symlink")
        seen.add(name.casefold())
        if info.file_size > 8 * 1024 * 1024 or (info.file_size > 1024 * 1024 and info.file_size > max(1, info.compress_size) * 200):
            raise ValueError("oversized member or compression bomb")
    return archive


def verify(root):
    files = [p for p in root.rglob("*") if p.is_file()]
    if len(files) > 2048 or any(p.is_symlink() for p in files):
        raise ValueError("library file count exceeds bound or includes symlinks")
    installed = sum(p.stat().st_size for p in files)
    if installed > QUOTA:
        raise ValueError("installed library exceeds 64MiB quota")
    if (root / "catalog.json").stat().st_size > 4 * 1024 * 1024:
        raise ValueError("catalog exceeds metadata bound")
    catalog = json.loads((root / "catalog.json").read_text(encoding="utf-8"))
    if catalog["schema_version"] != 1:
        raise ValueError("unsupported catalog version")
    ids, paths, counts, size = set(), set(), {}, 0
    for entry in catalog["entries"]:
        if entry["id"] in ids or entry["path"].casefold() in paths:
            raise ValueError("duplicate stable ID or path")
        ids.add(entry["id"])
        paths.add(entry["path"].casefold())
        path = confined(root, entry["path"])
        notice = confined(root, entry["license_path"])
        if path.is_symlink() or not path.is_file() or not notice.is_file():
            raise ValueError("missing asset/notice or symlink")
        data = path.read_bytes()
        if len(data) != entry["size_bytes"] or digest(data) != entry["sha256"]:
            raise ValueError(f"asset integrity failure: {entry['id']}")
        if entry["kind"] not in {"audio", "image", "svg", "font"} or entry["license_id"] not in {"CC0-1.0", "OFL-1.1"}:
            raise ValueError("unsupported kind or unreviewed asset license")
        if entry["kind"] == "svg" and re.search(rb"<!DOCTYPE|<!ENTITY|<script|href\s*=|<foreignObject", data, re.I):
            raise ValueError("external/scripted SVG rejected")
        size += len(data)
        counts[entry["kind"]] = counts.get(entry["kind"], 0) + 1
    manifest_path = root / "HASHES.json"
    if not manifest_path.is_file():
        raise ValueError("required full-file hash manifest is missing")
    if manifest_path.stat().st_size > 4 * 1024 * 1024:
        raise ValueError("hash manifest exceeds metadata bound")
    if manifest_path.exists():
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        expected = set(manifest["files"])
        actual = {p.relative_to(root).as_posix() for p in root.rglob("*") if p.is_file() and p != manifest_path}
        if expected != actual:
            raise ValueError("hash manifest file inventory mismatch")
        for name, expected_hash in manifest["files"].items():
            path = confined(root, name)
            if path.is_symlink() or digest(path.read_bytes()) != expected_hash:
                raise ValueError(f"metadata/media hash manifest failure: {name}")
    return {"status": "verified", "asset_count": len(ids), "counts": counts,
            "asset_bytes": size, "installed_bytes": installed,
            "catalog_sha256": digest((root / "catalog.json").read_bytes())}


def audit(root, output):
    verify(root)
    entries = json.loads((root / "catalog.json").read_text(encoding="utf-8"))["entries"]
    def inspect(entry):
        path = confined(root, entry["path"])
        if entry["kind"] == "audio":
            probe = subprocess.run(["ffprobe", "-v", "error", "-show_streams", "-show_format", "-of", "json", str(path)],
                                   capture_output=True, check=True, timeout=20)
            decoded = subprocess.run(["ffmpeg", "-v", "error", "-nostdin", "-i", str(path), "-f", "null", "-"],
                                     capture_output=True, check=True, timeout=30)
            if decoded.stderr.strip():
                raise ValueError(f"decoder diagnostic: {entry['id']}")
            result = json.loads(probe.stdout)
            stream = next(s for s in result["streams"] if s["codec_type"] == "audio")
            return {"id": entry["id"], "decoded": True, "codec": stream["codec_name"],
                    "sample_rate": int(stream["sample_rate"]), "channels": stream["channels"],
                    "duration_ms": round(float(result["format"]["duration"]) * 1000)}
        if entry["kind"] == "image":
            subprocess.run(["ffmpeg", "-v", "error", "-nostdin", "-i", str(path), "-f", "null", "-"],
                           capture_output=True, check=True, timeout=20)
            data = path.read_bytes()
            width, height = struct.unpack(">II", data[16:24])
            return {"id": entry["id"], "decoded": True, "width": width, "height": height}
        return {"id": entry["id"], "hash_checked": True, "decoded": False}
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        results = list(pool.map(inspect, entries))
    report = {"schema_version": 1, "verification": verify(root), "results": results}
    with output.open("x", encoding="utf-8") as writer:
        json.dump(report, writer, indent=2)
        writer.write("\n")
    print(json.dumps({"audit": str(output), "items": len(results), "media_decoded": sum(r.get("decoded", False) for r in results)}))


def acquire(root, destination):
    verify(root)
    if destination.exists():
        raise ValueError("destination exists; acquisition refuses overwrite")
    sources = json.loads((root / "sources.json").read_text(encoding="utf-8"))
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".library-stage-", dir=destination.parent) as temporary:
        stage = Path(temporary) / "library"
        stage.mkdir()
        # Metadata/license proofs are already reviewed, pinned and locally retained.
        for path in root.rglob("*"):
            if path.is_file() and path.suffix not in {".ogg", ".png", ".ttf"}:
                target = stage / path.relative_to(root)
                target.parent.mkdir(parents=True, exist_ok=True)
                with target.open("xb") as writer:
                    writer.write(path.read_bytes())
        for source in sources["downloads"]:
            data = download(source["url"], source["sha256"])
            archive = safe_archive(data) if source.get("members") else None
            members = source["members"] if archive else [{"path": source["path"]}]
            for member in members:
                target = confined(stage, member["path"])
                target.parent.mkdir(parents=True, exist_ok=True)
                blob = archive.read(member["member"]) if archive else data
                with target.open("xb") as writer:
                    writer.write(blob)
        verify(stage)
        # Windows MoveFile semantics reject an existing directory; no user files removed.
        if destination.exists():
            raise ValueError("destination appeared during acquisition")
        os.rename(stage, destination)
    print(json.dumps(verify(destination)))


def self_test(root):
    import unittest
    class SafetyTests(unittest.TestCase):
        def test_confined_paths(self):
            for name in ["../escape", "/absolute", "C:/outside", "a\\b", "asset:stream"]:
                with self.assertRaises(ValueError):
                    confined(root, name)

        def test_archive_paths_and_duplicates(self):
            for names in [["../escape.ogg"], ["A.ogg", "a.ogg"]]:
                buffer = io.BytesIO()
                with zipfile.ZipFile(buffer, "w") as writer:
                    for name in names:
                        writer.writestr(name, b"fixture")
                with self.assertRaises(ValueError):
                    safe_archive(buffer.getvalue())

        def test_archive_symlink(self):
            buffer = io.BytesIO()
            with zipfile.ZipFile(buffer, "w") as writer:
                link = zipfile.ZipInfo("link.ogg")
                link.external_attr = 0o120777 << 16
                writer.writestr(link, b"../outside")
            with self.assertRaises(ValueError):
                safe_archive(buffer.getvalue())

        def test_destination_noclobber(self):
            with tempfile.TemporaryDirectory() as temporary:
                destination = Path(temporary) / "existing"
                destination.mkdir()
                sentinel = destination / "keep.txt"
                sentinel.write_bytes(b"keep")
                with self.assertRaises(ValueError):
                    acquire(root, destination)
                self.assertEqual(sentinel.read_bytes(), b"keep")

        def test_asset_and_notice_tamper(self):
            with tempfile.TemporaryDirectory() as temporary:
                copied = Path(temporary) / "library"
                shutil.copytree(root, copied)
                catalog = json.loads((copied / "catalog.json").read_text(encoding="utf-8"))
                asset = confined(copied, catalog["entries"][0]["path"])
                original = asset.read_bytes()
                asset.write_bytes(original + b"tamper")
                with self.assertRaises(ValueError):
                    verify(copied)
                asset.write_bytes(original)
                notice = confined(copied, catalog["entries"][0]["license_path"])
                notice.write_bytes(b"license tamper")
                with self.assertRaises(ValueError):
                    verify(copied)

        def test_unreviewed_urls(self):
            for url in ["http://kenney.nl/file", "https://example.com/file", "https://user:secret@kenney.nl/file"]:
                with self.assertRaises(ValueError):
                    validate_url(url)

        def test_missing_hash_manifest(self):
            with tempfile.TemporaryDirectory() as temporary:
                copied = Path(temporary) / "library"
                shutil.copytree(root, copied)
                (copied / "HASHES.json").unlink()
                with self.assertRaises(ValueError):
                    verify(copied)
    result = unittest.TextTestRunner(verbosity=1).run(unittest.defaultTestLoader.loadTestsFromTestCase(SafetyTests))
    if not result.wasSuccessful():
        raise SystemExit(1)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=["verify", "audit", "acquire", "self-test"], nargs="?", default="verify")
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1] / "assets/library")
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.mode == "verify":
        print(json.dumps(verify(args.root)))
    elif args.mode == "self-test":
        self_test(args.root)
    elif args.output is None:
        parser.error("--output is required for audit/acquire and must be new")
    elif args.mode == "audit":
        audit(args.root, args.output)
    else:
        acquire(args.root, args.output)


if __name__ == "__main__":
    main()
