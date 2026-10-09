-- Storage migration 0 -> 1; project graph schema remains version 1.
CREATE VIRTUAL TABLE search_index USING fts5(object_id UNINDEXED, kind UNINDEXED, text, tokenize='unicode61');
PRAGMA user_version = 1;
