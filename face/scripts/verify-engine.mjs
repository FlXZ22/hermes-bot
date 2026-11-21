import fs from 'node:fs/promises';import path from 'node:path';import assert from 'node:assert/strict';import {createHash} from 'node:crypto';
import * as clone from '../demo/vendor/engine.js';
const originalPath='artifacts/app/dist/renderer/assets/index.eager-common-D2eZn7JB.js';
const source=await fs.readFile(originalPath,'utf8');
const provenance=JSON.parse(await fs.readFile('evidence/engine-provenance.json','utf8'));
const hash=s=>createHash('sha256').update(s).digest('hex');
assert.equal(hash(source),provenance.sha256);
for(const d of provenance.declarations)assert.equal(hash(source.slice(d.start,d.end)),d.sha256,`Unchanged declaration ${d.name}`);
await fs.mkdir('artifacts/reference',{recursive:true});
await fs.copyFile('artifacts/app/dist/renderer/assets/chunk-rolldown-runtime-BCiCTKAo.js','artifacts/reference/chunk-rolldown-runtime-BCiCTKAo.js');
await fs.writeFile('artifacts/reference/common.mjs',source+'\nexport {oo as createMotion,yo as createDirector,lo as clips,po as states,ci as expressions,oi as bodies,Qg as renderTree,Zg as renderSvg,c_ as appearance,gr as themeColors,x_ as themePrimitives};\n');
const original=await import(path.resolve('artifacts/reference/common.mjs'));
for(const key of ['clips','states','expressions','bodies','themeColors','themePrimitives'])assert.deepEqual(clone[key],original[key],key);
let frameChecks=0,svgChecks=0,scenarios=0;
function pair(body='sphere'){return [original.createMotion({body}),clone.createMotion({body})];}
function equal(a,b,label,svg=false){assert.deepEqual(b.frame(),a.frame(),label);frameChecks++;if(svg){assert.equal(clone.renderSvg(b.frame(),clone.appearance('#1e86fc'),192,'same-id'),original.renderSvg(a.frame(),original.appearance('#1e86fc'),192,'same-id'),label+' SVG');svgChecks++;}}
function run(motions,directors,seconds,label){scenarios++;for(let i=0;i<=Math.ceil(seconds*60);i++){if(i%30===0)equal(...motions,label+' @'+i/60,true);for(let n=0;n<2;n++){directors[n]?.advance(1/60);motions[n].advance(1/60);}equal(...motions,label+' frame '+i);}}
for(const state of Object.keys(clone.states)){const motions=pair();const directors=motions.map((m,i)=>(i?clone:original).createDirector(m,state,{seed:724,firstPhase:0}));run(motions,directors,state==='blocked'?123:12,`state ${state}`);}
for(const clip of clone.clips){const motions=pair();motions[0].load(original.clips.find(c=>c.name===clip.name));motions[1].load(clip);run(motions,[],clip.duration*2+.5,`clip ${clip.name}`);}
for(const body of clone.bodies){const motions=pair(body);for(const expr of clone.expressions){motions.forEach(m=>m.setExpression(expr));run(motions,[],.5,`${body} ${expr}`);}}
for(const from of clone.expressions)for(const to of clone.expressions){const motions=pair();motions.forEach(m=>{m.setExpression(from);for(let i=0;i<30;i++)m.advance(1/60);m.setExpression(to,{isThroughBlink:true});});run(motions,[],.6,`expression ${from} to ${to}`);}
for(const morph of Object.keys(clone.morphKinds)){const motions=pair();motions.forEach(m=>m.setMorph(morph));run(motions,[],6,`morph ${morph}`);motions.forEach(m=>m.setMorph('writing'));run(motions,[],1,`morph ${morph} to writing`);}
const motions=pair();for(const body of clone.bodies){motions.forEach(m=>m.setBody(body));run(motions,[],.5,`shape morph ${body}`);}
for(const poke of ['spin','bounce','nod']){motions.forEach(m=>m.poke(poke,{turns:2}));run(motions,[],2,`gesture ${poke}`);}
for(const look of Object.keys(clone.looks)){motions.forEach(m=>m.look(look));run(motions,[],1,`look ${look}`);}
motions.forEach(m=>{m.poke('spin',{turns:2});m.poke('bounce');m.blink();m.setGazeAlways(true);m.setGaze([.7,-.4]);});run(motions,[],3,'combined celebration, blink, gaze');
const report={result:'pass',sourceSha256:hash(source),unchangedDeclarations:provenance.declarations.length,frameChecks,svgChecks,scenarios,coverage:{states:Object.keys(clone.states).length,clips:clone.clips.length,expressions:clone.expressions.length,bodies:clone.bodies.length,morphs:Object.keys(clone.morphKinds).length},scope:'Differential evaluation of recovered declarations versus the complete original shipped common bundle. This does not compare the authenticated full desktop UI.'};
await fs.writeFile('evidence/verification.json',JSON.stringify(report,null,2));console.log(JSON.stringify(report,null,2));
