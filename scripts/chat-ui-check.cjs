const { chromium } = require(process.env.PLAYWRIGHT_MODULE || '@playwright/test');
const assert = require('node:assert/strict');
(async () => {
 const browser = await chromium.launch({headless:true});
 const page = await browser.newPage({viewport:{width:1200,height:850}});

 page.on('pageerror', e => console.error('PAGE ERROR',e));
 await page.goto('http://127.0.0.1:5173/qa/chat.html');
 await page.waitForSelector('.gb-entry');
 await page.waitForTimeout(700);
 const count=await page.locator('.gb-entry').count(); assert.ok(count < 45, `Mounted ${count} rows`);
 const scroller=page.locator('.gb-transcript');
 await scroller.evaluate(e=> { e.scrollTop=e.scrollHeight; });
 await page.waitForTimeout(200);
 await page.evaluate(()=>window.append('\n\n'+'Streamed content. '.repeat(100)));
 await page.waitForTimeout(300);
 const gap=await scroller.evaluate(e=>e.scrollHeight-e.scrollTop-e.clientHeight); assert.ok(gap<60, `Bottom gap ${gap}`);
 await scroller.evaluate(e=> {e.scrollTop-=1500;});
 await page.waitForTimeout(250);
 const before=await scroller.evaluate(e=>e.scrollTop);
 await page.evaluate(()=>window.append('\n\n'+'More streamed content. '.repeat(100)));
 await page.waitForTimeout(250);
 const after=await scroller.evaluate(e=>e.scrollTop); assert.ok(Math.abs(after-before)<5, `Scroll moved ${after-before}`);
 await page.getByRole('button',{name:'Jump to latest'}).click();
 await page.waitForTimeout(300);
 assert.ok(await page.getByText('Message 999',{exact:false}).isVisible());
 console.log(JSON.stringify({mountedRows:count,totalMessages:1000,bottomGap:gap,scrollDrift:after-before}));
 await page.evaluate(() => window.replaceLast('# Heading one\n\n## Heading two\n\n### Heading three\n\n#### Heading four\n\nParagraph with **bold**, *italic*, ~~removed~~ and `inline`.\n\n1. First\n   - Nested\n2. Second\n\n- [x] Task\n\n> A quote\n\n| Name | Value |\n| --- | --- |\n| Test | 42 |\n\n```js\nconst answer = 42;\nconsole.log(answer);\n```'));
 await page.locator('.gb-bubble-agent h1').waitFor();
 const last = page.locator('.gb-entry--assistant').last();
 assert.equal(await last.locator('h1,h2,h3,h4').count(), 4);
 assert.equal(await last.locator('table tbody tr').count(), 1);
 assert.equal(await last.locator('ol ul li').count(), 1);
 assert.equal(await last.locator('blockquote').count(), 1);
 await last.locator('.hljs-keyword').first().waitFor();
 assert.equal(await last.getByRole('button', {name:'Copy code'}).count(),1);
 assert.equal(await last.locator('.gb-thinking-card__head').getAttribute('aria-expanded'), 'false');
 assert.equal(await last.locator('.gb-tool-card__header').getAttribute('aria-expanded'), 'false');
 await last.locator('.gb-thinking-card__head').click();
 assert.equal(await last.locator('.gb-thinking-card__body strong').textContent(),'Reasoning');
 await last.locator('.gb-tool-card__header').click();
 await last.locator('.gb-tool-card__body .hljs-attr').first().waitFor();
 await last.locator('.gb-tool-card__header').click();
 assert.equal(await last.locator('.gb-tool-card__body').count(),0);
 console.log('PASS collapsed thinking/tool activity and formatted expanded payloads');
 await page.context().grantPermissions(['clipboard-read','clipboard-write']);
 await last.getByRole('button',{name:'Copy code',exact:true}).click();
 assert.equal(await page.evaluate(()=>navigator.clipboard.readText()),'const answer = 42;\nconsole.log(answer);');
 await page.evaluate(()=>{window.replaceLast('A **streaming response');window.streaming();});
 await last.locator('.gb-bubble-agent strong').waitFor();
 assert.equal(await last.locator('.gb-bubble-agent strong').textContent(),'streaming response');
 await page.evaluate(()=>window.replaceLast('```js\nconst value = 1;'));
 await last.locator('.gb-bubble-agent .gb-codeblock').waitFor();
 assert.match(await last.locator('.gb-bubble-agent pre code').textContent(),/const value = 1/);
 console.log('PASS code copying, incomplete emphasis and unclosed code fences');
 if (process.env.HERMES_QA_SCREENSHOTS) await page.screenshot({path:'analysis/chat-performance.png'});
 console.log('PASS GFM headings, nested lists, tables, quotes and syntax highlighting');
 let imageRequests = 0;
 await page.route('https://images.fixture/**', route => { imageRequests++; return route.fulfill({ contentType: 'image/svg+xml', body:'<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>' }); });
 await page.evaluate(() => window.replaceLast('# Worker\n\n' + 'A paragraph of text. '.repeat(250) + '\n\n| A | B |\n| - | - |\n| x | y |\n\n![remote](https://images.fixture/a.png)'));
 await page.locator('.gb-bubble-agent table').waitFor();
 await page.getByRole('button', {name:'Load image: remote'}).waitFor();
 assert.equal(imageRequests, 0);
 await page.getByRole('button', {name:'Load image: remote'}).click();
 await page.waitForFunction(() => document.querySelector('img[alt="remote"]')?.complete);
 assert.equal(imageRequests, 1);
 console.log('PASS large GFM response parsed in worker and remote image consent');
 const app = await browser.newPage({viewport:{width:1300,height:900}});
 const appErrors=[]; app.on('pageerror',error=>appErrors.push(error.message));
 const profiles=[{name:'default',active:true},{name:'writer'},{name:'third'},{name:'fourth'},{name:'fifth'},{name:'sixth'},{name:'seventh'}];
 let selectedProfile='default', rejectSwitch=false, socket;
 await app.addInitScript(()=>localStorage.setItem('hermes-grok-bot:connection',JSON.stringify({id:'fixture',name:'Fixture',url:'http://hermes.fixture',mode:'dashboard',authType:'none'})));
 await app.route('http://hermes.fixture/**', async route=>{
  const request=route.request(), url=new URL(request.url());
  let data={}; let status=200;
  if(url.pathname==='/api/profiles') {
    if(request.method()==='POST') profiles.push({...request.postDataJSON(),active:false});
    data={profiles};
  } else if(url.pathname==='/api/profiles/active') {
    if(rejectSwitch){status=500;data={detail:'Fixture switch rejected'};}
    else {selectedProfile=request.postDataJSON().name;data={ok:true};}
  } else if(url.pathname==='/api/sessions') data={sessions:[{id:`session-${selectedProfile}`,title:`${selectedProfile} conversation`,profile:selectedProfile,model:'test-model',context_limit:100000}]};
  else if(url.pathname.endsWith('/messages')) {
   const offset=Number(url.searchParams.get('offset') ?? 0);
   data={messages:offset===0 ? Array.from({length:48},(_,i)=>({id:`m-${i}`,
    role:i%2===0?'user':'assistant',content:i%2===0?`Prompt ${i}`:`## Ready ${i}\n\nA **formatted** answer.`}))
    : [{id:'historical',role:'assistant',content:'Older response'}]};
  }
  else if(url.pathname==='/api/status') data={status:'running',model:'test-model'};
  else if(url.pathname.endsWith('/soul')) data={content:'# Soul'};
  await route.fulfill({status,contentType:'application/json',headers:{'Access-Control-Allow-Origin':'*'},body:JSON.stringify(data)});
 });
 await app.routeWebSocket('ws://hermes.fixture/**',ws=>{
   socket=ws;
   ws.onMessage(raw=>{const msg=JSON.parse(raw.toString());ws.send(JSON.stringify({jsonrpc:'2.0',id:msg.id,result:msg.method==='session.resume'?{session_id:'live-fixture'}:{}}));});
 });
 await app.goto('http://127.0.0.1:5173/');
 await app.getByRole('button',{name:'Add bot',exact:true}).waitFor();
 await app.waitForFunction(()=>document.querySelectorAll('.gb-pinned-tile').length===7);
 await app.locator('.gb-animated-face__renderer svg').first().waitFor();
 await app.waitForTimeout(400);
 if (process.env.HERMES_QA_SCREENSHOTS) await app.locator('.gb-welcome').screenshot({path:'analysis/chat-welcome.png'});
 await app.getByRole('button',{name:'Add bot',exact:true}).click();
 await app.getByRole('textbox',{name:'Bot name'}).fill('research');
 await app.getByRole('textbox',{name:'Description',exact:true}).fill('Research assistant');
 await app.getByRole('button',{name:'Create bot',exact:true}).click();
 await app.waitForFunction(()=>document.querySelector('.gb-pinned-tile[data-active=true]')?.textContent?.includes('research'));
 assert.equal(selectedProfile,'research');
 assert.equal(await app.locator('.gb-pinned-tile').count(),8);
 await app.locator('.gb-pinned-tile').filter({hasText:'writer'}).click();
 await app.waitForFunction(()=>document.querySelector('.gb-pinned-tile[data-active=true]')?.textContent?.includes('writer'));
 rejectSwitch=true;
 await app.locator('.gb-pinned-tile').filter({hasText:'Hermes'}).click();
 await app.getByText('Fixture switch rejected',{exact:true}).waitFor();
 assert.match(await app.locator('.gb-pinned-tile[data-active=true]').innerText(),/writer/);
 rejectSwitch=false;
 await app.locator('.gb-agent-row').filter({hasText:'writer conversation'}).click();
 await app.getByRole('heading',{name:'Ready 47',exact:true}).waitFor();
 await app.locator('.gb-transcript').evaluate(element=>{element.scrollTop=0;});
 await app.getByRole('button',{name:'Load older messages',exact:true}).click();
 await app.waitForFunction(()=>!document.body.textContent.includes('Load older messages'));
 await app.locator('.gb-transcript').evaluate(element=>{element.scrollTop=0;});
 await app.getByText('Older response',{exact:true}).waitFor();
 await app.getByRole('button',{name:'Jump to latest'}).click();
 await app.getByRole('heading',{name:'Ready 47',exact:true}).waitFor();
 console.log('PASS older history pagination preserves the latest messages');
 socket.send(JSON.stringify({jsonrpc:'2.0',method:'event',params:{type:'session.usage',session_id:'live-fixture',payload:{context_tokens:25000,context_limit:100000,total_tokens:80000}}}));
 await app.getByText('Context 25.0k / 100.0k',{exact:true}).waitFor();
 assert.equal(await app.getByRole('progressbar',{name:'Context usage'}).getAttribute('aria-valuenow'),'25');
 // Real event bursts exercise the batching hook and final-message ordering.
 socket.send(JSON.stringify({method:'event',params:{type:'message.start',session_id:'live-fixture',payload:{}}}));
 for(let i=0;i<100;i++)socket.send(JSON.stringify({method:'event',params:{type:'text.delta',session_id:'live-fixture',payload:{delta:'word '}}}));
 socket.send(JSON.stringify({method:'event',params:{type:'message.complete',session_id:'live-fixture',payload:{}}}));
 await app.waitForFunction(()=>[...document.querySelectorAll('.gb-bubble-agent')].some(e=>e.textContent.trim()==='word '.repeat(100).trim()));
 await app.getByRole('button',{name:'Send message (Enter)',exact:true}).waitFor();
 assert.deepEqual(appErrors,[]);
 if (process.env.HERMES_QA_SCREENSHOTS) await app.screenshot({path:'analysis/chat-finished.png'});
 await app.emulateMedia({reducedMotion:'reduce'});
 await app.waitForTimeout(150);
 const face=app.locator('.gb-animated-face__renderer svg').first();
 const frozen=await face.evaluate(el=>el.outerHTML);
 await app.waitForTimeout(200);
 assert.equal(await face.evaluate(el=>el.outerHTML),frozen);
 console.log('PASS reference renderer respects reduced motion');
 console.log('PASS full app: 8 bots, create/switch, rejected-switch recovery, usage and 100-token burst ordering; no page errors');

 // Activity presentation: lazy payloads, semantic icons, one live animation and
 // correct transitions between file writing, terminal work and completion.
 await page.evaluate(() => window.setActivity([
  {id:'read',name:'read_file',arguments:{path:'pasted-text-1.txt'},status:'done'},
  {id:'run',name:'terminal',arguments:{command:'rg --files src scripts'},output:'terminal output',status:'done'},
  {id:'write',name:'write_file',arguments:{path:'src/app.ts'},status:'running'},
 ], true));
 const activity = page.locator('.gb-entry--assistant').last();
 await activity.locator('[data-grok-state="writing"][data-engine-ready="true"]').waitFor();
 assert.equal(await activity.locator('.gb-animated-face').count(),1);
 assert.equal(await activity.locator('.gb-tool-card__body').count(),0);
 assert.equal(await activity.locator('.gb-tool-card__header').nth(0).textContent().then(t=>t.includes('pasted-text-1.txt')),true);
 assert.equal(await activity.locator('.gb-tool-card__header').nth(1).textContent().then(t=>t.includes('rg --files')),true);
 for (const selector of ['.gb-thinking-card','.gb-tool-card']) {
  assert.equal(await activity.locator(selector).first().evaluate(el=>getComputedStyle(el).borderTopWidth),'0px');
 }
 const summary=activity.locator('.gb-activity-summary');
 await summary.focus(); await page.keyboard.press('Enter');
 assert.equal(await activity.locator('.gb-tool-card').count(),0);
 await page.keyboard.press('Enter');
 await activity.locator('.gb-tool-card__header').nth(1).click();
 await activity.getByText('terminal output',{exact:true}).waitFor();
 await page.evaluate(() => window.setActivity([{id:'write',name:'terminal',arguments:'{"command":"npm test"}',status:'running'}],true));
 await activity.locator('[data-grok-state="running"]').waitFor();
 await page.evaluate(() => window.setActivity([{id:'write',name:'terminal',arguments:{command:'npm test'},status:'error'}],false));
 await activity.getByText('Failed',{exact:true}).waitFor();
 assert.equal(await activity.locator('.gb-animated-face').count(),0);
 await page.setViewportSize({width:390,height:844});
 assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth > innerWidth),false);
 console.log('PASS minimal activity rows, keyboard disclosure, lazy payloads, writing/terminal animation transitions, failure and mobile layout');
 await browser.close();
})().catch(e=>{console.error(e);process.exit(1)});
