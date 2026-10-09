"""Original presentation fixtures, not a native-scene renderer. Requires local Pillow/FFmpeg."""
from pathlib import Path
import argparse, json, math, os, subprocess, uuid
from PIL import Image, ImageDraw, ImageFont

parser = argparse.ArgumentParser()
parser.add_argument('--cli', required=True)
parser.add_argument('--output', required=True)
args = parser.parse_args()
root = Path(args.output).resolve()
root.mkdir(parents=True, exist_ok=False)
assets = root / 'assets'
assets.mkdir()
W,H = 960,540
MINT=(108,245,207); BLUE=(79,159,250); PURPLE=(170,156,250)
font_path=Path(os.environ.get('WINDIR','C:/Windows'))/'Fonts'/'segoeui.ttf'
def font(size):
    return ImageFont.truetype(str(font_path),size) if font_path.exists() else ImageFont.load_default(size=size)
def base():
    image=Image.new('RGB',(W,H))
    pixels=image.load()
    for y in range(H):
        for x in range(W):
            glow=max(0,1-math.hypot((x-750)/950,(y-70)/620))
            pixels[x,y]=(int(4+glow*6),int(13+glow*10),int(27+glow*28))
    return image
def center(draw,text,y,size,color=(244,246,254)):
    f=font(size); box=draw.textbbox((0,0),text,font=f)
    draw.text(((W-(box[2]-box[0]))/2,y),text,font=f,fill=color)
def network(image,mode):
    d=ImageDraw.Draw(image)
    columns=[[(222,180+72*i) for i in range(4)],[(490,180+72*i) for i in range(4)],[(745,215+81*i) for i in range(3)]]
    shown=columns if mode==2 else columns[:2] if mode==1 else columns[1:]
    colors=[MINT,BLUE,PURPLE] if mode==2 else [MINT,BLUE] if mode==1 else [BLUE,PURPLE]
    for i in range(len(shown)-1):
        for a in shown[i]:
            for b in shown[i+1]:d.line([a,b],fill=(39,101,142),width=1)
    for nodes,color in zip(shown,colors):
        for x,y in nodes:
            for radius in range(28,19,-1):
                d.ellipse((x-radius,y-radius,x+radius,y+radius),fill=tuple(int(c*(0.2+(28-radius)*.025)) for c in color))
            d.ellipse((x-20,y-20,x+20,y+20),fill=color,outline=tuple(min(255,int(c*1.3)) for c in color),width=2)
    labels=['Input','Hidden layers','Output'] if mode==2 else ['Input','Hidden layers'] if mode==1 else ['Hidden layers','Output']
    for nodes,label in zip(shown,labels):
        f=font(18);box=d.textbbox((0,0),label,font=f);d.text((nodes[0][0]-(box[2]-box[0])/2,459),label,font=f,fill=(225,234,250))
def artwork(index,title):
    image=base();d=ImageDraw.Draw(image)
    if index in [1,2,3]:
        center(d,'From inputs to answers' if index==2 else 'Every connection matters' if index==1 else 'A useful prediction',68,32)
        network(image,index)
    elif index==0:
        center(d,'Neural networks',95,48);center(d,'Small connections. Remarkable possibilities.',158,21,(176,199,224))
        for i in range(17):
            a=(110+i*43,320+int(math.sin(i*.8)*60));b=(110+(i+1)*43,320+int(math.sin((i+1)*.8)*60))
            d.line([a,b],fill=(32,103,143),width=2);d.ellipse((a[0]-5,a[1]-5,a[0]+5,a[1]+5),fill=MINT if i%3==0 else BLUE)
    elif index==4:
        center(d,'Learn. Connect. Predict.',112,40)
        for x,color,label in [(230,MINT,'Inputs'),(480,BLUE,'Connections'),(730,PURPLE,'Answers')]:
            d.rounded_rectangle((x-85,265,x+85,390),radius=16,fill=(19,39,61),outline=color,width=2)
            d.ellipse((x-19,286,x+19,324),fill=color)
            f=font(17);box=d.textbbox((0,0),label,font=f);d.text((x-(box[2]-box[0])/2,345),label,font=f,fill=color)
    elif index==5:
        for i in range(4):
            ridge=[(x,275+i*50+int(math.sin(x*.008+i*2)*40)) for x in range(0,W+1,8)]
            d.polygon(ridge+[(W,H),(0,H)],fill=[(37,58,99),(40,79,120),(34,102,128),(31,130,137)][i])
        center(d,'Ideas in motion',74,38)
    elif index==6:
        for x in range(100,880,80):
            for y in range(140,460,70):d.rounded_rectangle((x,y,x+50,y+30),radius=5,fill=(12,44,64),outline=(36,125,159),width=1)
        center(d,'Data visualization',63,34)
    else:
        for x in range(90,900,5):
            amplitude=int((45+25*math.sin(x*.025))*abs(math.sin(x*.13)))
            d.line([(x,330-amplitude),(x,330+amplitude)],fill=BLUE,width=2)
        center(d,'Sound shapes the story',92,38)
    return image
names=['Opening','Inputs','Hidden layers','Output','Recap','Graphics','Data visualization','Sound design']
durations=[10,14,20,10,6,4,4,4]
media_ids=[str(uuid.uuid4()) for _ in names]
for i,(name,duration) in enumerate(zip(names,durations)):
    png=assets/(name+'.png');video=assets/(name+'.mkv');artwork(i,name).save(png)
    cmd=[os.environ.get('LUMERYN_FFMPEG','ffmpeg'),'-v','error','-nostdin','-n','-loop','1','-framerate','30','-i',str(png),'-f','lavfi','-i',f'sine=frequency={180+i*60}:sample_rate=48000:duration={duration}','-t',str(duration),'-vf','setsar=1,format=yuv420p','-af','volume=0.08+0.3*abs(sin(t*2)):eval=frame','-c:v','ffv1','-threads','2','-c:a','pcm_s16le',str(video)]
    subprocess.run(cmd,check=True,timeout=120)
project=root/'project.lumeryn'
def cli(*arguments):
    result=subprocess.run([args.cli,*arguments],capture_output=True,text=True,timeout=300)
    if result.returncode:raise RuntimeError(result.stderr or result.stdout)
    return json.loads(result.stdout)['result']
cli('project','create','--project',str(project),'--name','Neural networks','--width','1280','--height','720','--fps','30/1')
imports={'operations':[{'operation':'import','id':ident,'path':str(assets/(name+'.mkv'))} for ident,name in zip(media_ids,names)]}
import_file=root/'imports.json';import_file.write_text(json.dumps(imports),encoding='utf-8')
cli('project','batch','--project',str(project),'--file',str(import_file),'--expected-revision','0','--dry-run')
cli('project','batch','--project',str(project),'--file',str(import_file),'--expected-revision','0','--idempotency-key','studio-demo-imports')
clips=[];at=0
for ident,duration in zip(media_ids[:5],durations[:5]):
    clips.append({'operation':'add_clip','id':str(uuid.uuid4()),'media_id':ident,'at_frame':at,'source_in_frame':0,'duration_frames':duration*30});at+=duration*30
batch_file=root/'clips.json';batch_file.write_text(json.dumps({'operations':clips}),encoding='utf-8')
cli('project','batch','--project',str(project),'--file',str(batch_file),'--expected-revision','1','--dry-run')
cli('project','batch','--project',str(project),'--file',str(batch_file),'--expected-revision','1','--idempotency-key','studio-demo-clips')
integrity=cli('project','check','--project',str(project),'--assets')
(root/'PROVENANCE.md').write_text('Original locally drawn fixture artwork and synthetic modulated sine audio. No downloaded stock or model. System font rasterized on this Windows host; font files are not bundled. Imported raster footage demonstrates the real timeline and UI; it does not demonstrate editable native scenes, narration, transitions or semantic AI. Generated originals stay unchanged.\n',encoding='utf-8')
print(json.dumps({'project':str(project),'integrity':integrity,'duration_seconds':60,'media_assets':8}))
