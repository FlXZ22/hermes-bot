import * as E from './vendor/engine.js';
const $=id=>document.getElementById(id);
const title=s=>s.replace(/[_-]/g,' ').replace(/\b\w/g,c=>c.toUpperCase());
const groups={Lifecycle:E.stateNames.slice(0,17),Reactions:E.stateNames.slice(17,34),Morphs:E.stateNames.slice(34,47),Product:E.stateNames.slice(47,50),Brand:E.stateNames.slice(50)};
const params=new URLSearchParams(location.search);
let mode=['states','expressions','clips','morphs'].includes(params.get('mode'))?params.get('mode'):'states';
let selected=params.get('item')||'thinking';
let color=E.colors.some(c=>c.id===params.get('color'))?params.get('color'):'black';
let body=E.bodies.includes(params.get('body'))?params.get('body'):'sphere';
let theme=params.get('theme')==='dark'?'dark':'light';
let playing=!matchMedia('(prefers-reduced-motion: reduce)').matches,speed=1,motion,director,last=0,uiTick=0;
const fill=E.fillScale(E.stillFill);
const ink=(id=color)=>['black','gray'].includes(id)?E.themeColors[id][theme]:E.themePrimitives[theme][`${id==='violet'?'purple':id}/0`];
const choices=()=>mode==='states'?Object.keys(E.states):mode==='expressions'?E.expressions:mode==='clips'?E.clips.map(c=>c.name):Object.keys(E.morphKinds);
if(!choices().includes(selected))selected=choices()[0];
function init(){
 motion=E.createMotion({body});director=null;
 if(mode==='states')director=E.createDirector(motion,selected,{seed:724,firstPhase:0});
 else if(mode==='clips')motion.load(E.clips.find(c=>c.name===selected),{phase:0});
 else if(mode==='expressions'){motion.setExpression(selected);motion.play();}
 else {director=E.createDirector(motion,'idle',{seed:724,firstPhase:0});motion.setMorph(selected);}
 motion.setGazeAlways($('follow').checked);
 if(director)director.setEmphasis($('emphasis').checked);
 render();updateInfo();
}
function render(){
 const size=$('face').clientWidth||350;
 E.paintTree($('face'),E.renderTree(motion.frame(),E.appearance(ink()),size*fill,'review-face',{displaySize:size,extent:'100%'}));
 const svg=$('face').firstElementChild;
 svg.style.width=`${fill*100}%`;svg.style.height=`${fill*100}%`;svg.style.transform=`translate(${(1/fill-1)*50}%, ${(1/fill-1)*50}%)`;
}
function advance(dt){director?.advance(dt);motion.advance(dt);}
function updateInfo(){
 $('current-name').textContent=title(selected);
 const recipe=E.states[selected];
 const desc=mode==='states'?[recipe?.morph&&`${title(E.morphKinds[recipe.morph]||recipe.morph)} morph`,recipe?.face&&`${title(recipe.face)} eyes`,recipe?.next&&`Then ${title(recipe.next)}`].filter(Boolean).join(' · ')||recipe?.clips.join(' / '):mode==='clips'?'Original body, eye, and lid keyframes':mode==='expressions'?'Original eye geometry · switch expressions to morph':`${title(E.morphKinds[selected])} transformation`;
 $('current-description').textContent=desc;
 $('mode-label').textContent=mode==='states'?Object.entries(groups).find(([,v])=>v.includes(selected))?.[0]||'State':mode==='clips'?'Authored clip':title(mode);
 $('live-label').textContent=`${playing?'Playing':'Paused'} · ${speed}×`;
 $('play').textContent=playing?'Pause':'Play';
 document.querySelectorAll('[data-item]').forEach(el=>el.setAttribute('aria-pressed',el.dataset.item===selected));
 document.querySelectorAll('[data-tab]').forEach(el=>el.setAttribute('aria-selected',el.dataset.tab===mode));
 const p=new URLSearchParams({mode,item:selected,body,color,theme});history.replaceState(null,'',`?${p}`);
}
function choose(value){
 const previous=mode;selected=value;
 if(previous==='states'&&director){director.setState(value);}
 else if(mode==='expressions'){motion.setExpression(value,{isThroughBlink:$('blink-transition').checked});}
 else if(mode==='clips'){director=null;motion.holdFace(null);motion.setMorph(null);motion.load(E.clips.find(c=>c.name===value));}
 else if(mode==='morphs')motion.setMorph(value);
 else init();
 updateInfo();render();
}
function snapshot(item,kind,id){
 const m=E.createMotion({body});let d;
 if(kind==='states')d=E.createDirector(m,item,{seed:724,firstPhase:0});
 else if(kind==='clips')m.load(E.clips.find(c=>c.name===item),{phase:0});
 else if(kind==='expressions')m.setExpression(item);
 else{d=E.createDirector(m,'idle',{seed:724,firstPhase:0});m.setMorph(item);}
 for(let i=0;i<36;i++){d?.advance(1/30);m.advance(1/30);}
 return E.renderSvg(m.frame(),E.appearance(ink()),110,id,{displaySize:72});
}
let catalogToken=0;
async function catalog(){
 const token=++catalogToken,query=$('search').value.toLowerCase().trim();
 const root=$('catalog-content');root.replaceChildren();
 const source=mode==='states'?groups:{[mode==='clips'?'All authored clips':title(mode)]:choices()};
 let count=0;
 for(const [group,items] of Object.entries(source)){
  const matches=items.filter(i=>`${title(i)} ${E.morphKinds[i]||''} ${i==='coding'?'editing':''}`.toLowerCase().includes(query));if(!matches.length)continue;
  const heading=document.createElement('h3');heading.className='group-heading';heading.textContent=group;
  const badge=document.createElement('span');badge.textContent=matches.length;heading.append(badge);root.append(heading);
  const grid=document.createElement('div');grid.className='grid';root.append(grid);
  for(const item of matches){
   const tile=document.createElement('button');tile.className='tile';tile.dataset.item=item;tile.setAttribute('aria-pressed',item===selected);tile.title=`Preview ${title(item)}`;
   const thumb=document.createElement('div');thumb.className='thumbnail';thumb.setAttribute('aria-hidden','true');
   const label=document.createElement('span');label.className='tile-label';label.textContent=title(item);
   const sub=document.createElement('span');sub.className='tile-sub';sub.textContent=mode==='clips'?`${E.clips.find(c=>c.name===item).duration.toFixed(1)} seconds`:mode==='states'?E.states[item].morph?`${title(E.morphKinds[E.states[item].morph]||E.states[item].morph)} morph`:E.states[item].face?`${title(E.states[item].face)} eyes`:`${new Set(E.states[item].clips).size} clip${new Set(E.states[item].clips).size>1?'s':''}`:mode==='morphs'?title(E.morphKinds[item]):'Eye expression';
   tile.append(thumb,label,sub);tile.onclick=()=>choose(item);grid.append(tile);count++;
   // Yield between thumbnails to keep the original live engine responsive.
   await new Promise(requestAnimationFrame);if(token!==catalogToken)return;
   thumb.innerHTML=snapshot(item,mode,`thumb-${mode}-${item}`);
  }
 }
 if(!count){const p=document.createElement('p');p.className='empty';p.textContent='No matching animations.';root.append(p);}
}
function applyTheme(){
 document.body.classList.toggle('dark',theme==='dark');$('theme').textContent=theme==='dark'?'Light theme':'Dark theme';$('theme').setAttribute('aria-label',theme==='dark'?'Switch to light theme':'Switch to dark theme');
 $('hex').textContent=ink();$('color-name').textContent=title(color);
 for(const b of $('colors').children){b.style.background=ink(b.dataset.color);b.setAttribute('aria-pressed',b.dataset.color===color);}
 render();updateInfo();catalog();
}
for(const shape of E.bodies){const o=document.createElement('option');o.value=shape;o.textContent=title(shape==='sphere'?'blob':shape==='heartChubby'?'heart':shape==='starFlower'?'flower':shape==='star6'?'star':shape);$('body').append(o);}$('body').value=body;
for(const c of E.colors){const b=document.createElement('button');b.dataset.color=c.id;b.title=c.label;b.setAttribute('aria-label',c.label);b.onclick=()=>{color=c.id;applyTheme();};$('colors').append(b);}
$('body').onchange=e=>{body=e.target.value;motion.setBody(body);updateInfo();catalog();render();};
$('theme').onclick=()=>{theme=theme==='light'?'dark':'light';applyTheme();};
$('follow').onchange=e=>motion.setGazeAlways(e.target.checked);
window.addEventListener('pointermove',e=>{if(!$('follow').checked)return;const r=$('face').getBoundingClientRect();motion.setGaze([Math.max(-1,Math.min(1,(e.clientX-r.left-r.width/2)/(r.width*2))),Math.max(-1,Math.min(1,-(e.clientY-r.top-r.height/2)/(r.width*2)))]);});
$('emphasis').onchange=e=>{director?.setEmphasis(e.target.checked);};
$('gestures').addEventListener('click',e=>{if(e.target.dataset.poke)motion.poke(e.target.dataset.poke);});
$('blink').onclick=()=>motion.blink();$('look').onchange=e=>motion.look(e.target.value);
$('play').onclick=()=>{playing=!playing;updateInfo();};
$('restart').onclick=()=>init();$('speed').onchange=e=>{speed=Number(e.target.value);updateInfo();};
$('timeline').oninput=e=>{playing=false;const target=Number(e.target.value)*motion.duration;init();for(let t=0;t<target;t+=1/120)advance(Math.min(1/120,target-t));render();updateInfo();};
$('search').oninput=()=>catalog();
document.querySelectorAll('[data-tab]').forEach(button=>button.onclick=()=>{mode=button.dataset.tab;selected=choices()[0];$('search').value='';init();catalog();});
new ResizeObserver(()=>render()).observe($('face'));
function frame(now){const dt=Math.min((now-last)/1000||0,0.05)*speed;last=now;if(playing&&!document.hidden)advance(dt);render();if(now-uiTick>80){$('timeline').value=motion.time/motion.duration||0;$('time').textContent=`${motion.time.toFixed(2)} / ${motion.duration.toFixed(2)}s`;uiTick=now;}requestAnimationFrame(frame);}
init();applyTheme();requestAnimationFrame(frame);
// Read-only inspection hook used by browser QA and available for reviewer diagnostics.
window.faceReview={engine:E,get selection(){return{mode,selected,body,color,theme,playing};},get frame(){return motion.frame();},choose};
