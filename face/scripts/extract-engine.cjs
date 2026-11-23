const fs=require('node:fs'),crypto=require('node:crypto');
const parser=require('../../node_modules/@babel/parser');
const traverse=require('../../node_modules/@babel/traverse').default;
const file='artifacts/app/dist/renderer/assets/index.eager-common-D2eZn7JB.js';
const source=fs.readFileSync(file,'utf8');
const ast=parser.parse(source,{sourceType:'module'});
let program;traverse(ast,{Program(p){program=p;p.stop();}});
const exportsMap={h_:'createHost',oo:'createMotion',yo:'createDirector',lo:'clips',po:'states',ci:'expressions',oi:'bodies',hn:'shapes',dn:'colors',gr:'themeColors',o_:'bodyForShape',c_:'appearance',Qg:'renderTree',n_:'paintTree',Zg:'renderSvg',co:'fillScale',so:'stillFill',Ma:'defaultFill',na:'pokes',Ya:'looks',mn:'stateNames',ya:'morphKinds',x_:'themePrimitives'};
const selected=new Map(),queue=Object.keys(exportsMap),missing=new Set();
while(queue.length){const name=queue.pop();const binding=program.scope.getBinding(name);if(!binding)throw Error('Missing root '+name);const p=binding.path;
if(selected.has(p.node.start))continue;
if(p.isImportSpecifier())throw Error('External dependency '+name);
selected.set(p.node.start,p);
p.traverse({ReferencedIdentifier(ref){const b=ref.scope.getBinding(ref.node.name);if(b&&b.scope===program.scope&&!selected.has(b.path.node.start))queue.push(ref.node.name);else if(!b)missing.add(ref.node.name);}});
}
const parts=[...selected.values()].sort((a,b)=>a.node.start-b.node.start);
let code='// Recovered unchanged declarations from Grok Bot 1.2.3. See evidence/engine-provenance.json.\n';
const ranges=[];for(const p of parts){const s=source.slice(p.node.start,p.node.end);code+=(p.isVariableDeclarator()?p.parent.kind+' '+s+';':s)+'\n';ranges.push({name:p.node.id?.name,start:p.node.start,end:p.node.end,sha256:crypto.createHash('sha256').update(s).digest('hex')});}
code+='export {'+Object.entries(exportsMap).map(([k,v])=>`${k} as ${v}`).join(',')+'};\n';
fs.writeFileSync('demo/vendor/engine.js',code);
fs.writeFileSync('evidence/engine-provenance.json',JSON.stringify({source:file,sha256:crypto.createHash('sha256').update(source).digest('hex'),declarations:ranges,globals:[...missing].sort()},null,2));
console.log({declarations:parts.length,bytes:code.length,globals:[...missing].sort()});
