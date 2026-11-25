// Synthetic source only. No host display, input, clipboard, or permission dialogs.
const {chromium} = require('/home/asad/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright');
const fs = require('fs'); const os = require('os'); const path = require('path'); const net = require('net'); const {spawn} = require('child_process'); const assert = require('assert/strict');
const sleep = ms => new Promise(r=>setTimeout(r,ms));
async function until(fn,ms=10000) { const end=Date.now()+ms;while(Date.now()<end){const result=await fn();if(result)return result;await sleep(25);}throw Error('deadline'); }
function request(port,request) {return new Promise((resolve,reject)=>{let output='';const s=net.connect({host:'127.0.0.1',port},()=>s.write(request));s.on('data',b=>output+=b.toString());s.on('end',()=>resolve(output));s.on('error',e=>output&&e.code==='ECONNRESET'?resolve(output):reject(e));s.setTimeout(6000,()=>{s.destroy();reject(Error('HTTP deadline'));});});}
function wsHeaders(port,origin=`http://127.0.0.1:${port}`,target='/websockify') {return `GET ${target} HTTP/1.1\r\nHost: 127.0.0.1:${port}\r\nOrigin: ${origin}\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: AAECAwQFBgcICQoLDA0ODw==\r\n\r\n`;}
async function run(control) {
 const dir=fs.mkdtempSync(path.join(os.tmpdir(),'termcraft-vnc-browser-'));const profile=path.join(dir,'profile');let browser;let test;let stdout='';
 try {
  test=spawn(process.argv[2],['services::vnc::browser::tests::synthetic_browser_fixture','--ignored','--exact','--nocapture','--test-threads=1'],{env:{...process.env,TERMCRAFT_BROWSER_FIXTURE:dir,TERMCRAFT_BROWSER_CONTROL:control?'1':'0'},stdio:['ignore','pipe','pipe']});
  const exited=new Promise((resolve,reject)=>{test.on('exit',code=>resolve(code));test.on('error',reject);});test.stdout.on('data',b=>stdout+=b);test.stderr.on('data',b=>stdout+=b);
  await until(()=>fs.existsSync(path.join(dir,'ready')));const [rfb,address]=fs.readFileSync(path.join(dir,'ready'),'utf8').trim().split('\n');const port=Number(address.split(':').pop());const url=`http://${address}/`;
  const events=()=>fs.existsSync(path.join(dir,'events'))?fs.readFileSync(path.join(dir,'events'),'utf8'):'';
  const status = await request(port,`GET / HTTP/1.1\r\nHost: ${address}\r\n\r\n`);assert(status.startsWith('HTTP/1.1 200'));assert(status.includes("Content-Security-Policy: default-src 'none'"));assert(status.includes('Cache-Control: no-store'));assert(!status.includes('Access-Control-Allow-Origin'));
  for(const [target,expected] of [['/?password=secret','404'],['/websockify?host=192.0.2.1','404'],['/novnc/%2e%2e/Cargo.toml','404'],['/missing','404']]) {const response=await request(port,`GET ${target} HTTP/1.1\r\nHost: ${address}\r\n\r\n`);assert(response.startsWith(`HTTP/1.1 ${expected}`));}
  assert((await request(port,`GET / HTTP/1.1\r\nHost: attacker.invalid:${port}\r\n\r\n`)).startsWith('HTTP/1.1 403'));
  assert((await request(port,wsHeaders(port,'https://attacker.invalid'))).startsWith('HTTP/1.1 403'));
  assert((await request(port,`OPTIONS / HTTP/1.1\r\nHost: ${address}\r\n\r\n`)).startsWith('HTTP/1.1 405'));
  assert((await request(port,`GET / HTTP/1.1\r\nHost: ${address}\r\nX-Pad: ${'x'.repeat(8300)}\r\n\r\n`)).startsWith('HTTP/1.1 431'));
  browser=await chromium.launchPersistentContext(profile,{executablePath:'/usr/bin/chromium',headless:true,viewport:{width:900,height:700},args:['--disable-background-networking','--disable-component-update','--no-first-run']});
  const page=await browser.newPage();const requests=[];const errors=[];page.on('request',r=>requests.push(r.url()));page.on('pageerror',e=>errors.push(e.message));
  await page.goto(url);await page.locator('#password').fill('wrong');await page.locator('#connect').click();await until(async()=>!(await page.locator('#connect').isDisabled()));assert(!await page.locator('#status').innerText().then(t=>t.startsWith('Connected')));assert.equal(await page.locator('#password').inputValue(),'');
  await page.locator('#password').fill('fixture');await page.locator('#connect').click();await page.waitForFunction(()=>document.getElementById('status').textContent==='Connected (view only)');
  await page.waitForFunction(()=>{const c=document.querySelector('canvas');if(!c||c.width!==64||c.height!==48)return false;return [...c.getContext('2d').getImageData(20,20,1,1).data].join(',')==='19,111,173,255';});
  if(process.env.TERMCRAFT_BROWSER_SCREENSHOT) await page.screenshot({path:process.env.TERMCRAFT_BROWSER_SCREENSHOT+`-${control?'control':'viewonly'}.png`});
  const canvas=page.locator('canvas');await canvas.click({position:{x:20,y:20}});await page.keyboard.press('a');await sleep(250);assert(!events().includes('key '));assert(!events().includes('pointer '));
  await page.locator('#viewOnly').uncheck();await canvas.click({position:{x:20,y:20}});await page.keyboard.press('b');
  if(control) await until(()=>events().includes('key true 98')&&events().includes('pointer 1 '));else {await sleep(250);assert(!events().includes('key '));assert(!events().includes('pointer '));}
  await page.locator('#disconnect').click();await page.waitForFunction(()=>document.getElementById('status').textContent==='Disconnected');
  assert.equal(await page.evaluate(()=>localStorage.length),0);assert.equal(await page.evaluate(()=>sessionStorage.length),0);assert(requests.every(u=>u.startsWith(url)||u===`ws://${address}/websockify`));assert(requests.every(u=>!u.includes('fixture')&&!u.includes('wrong')));assert.deepEqual(errors,[]);
  // Actual browser rejects an oversized binary message; gateway closes only this client.
  const oversized=await page.evaluate(()=>new Promise(resolve=>{const s=new WebSocket(`ws://${location.host}/websockify`);s.binaryType='arraybuffer';s.onopen=()=>s.send(new Uint8Array(1048577));s.onclose=()=>resolve(true);setTimeout(()=>resolve(false),4000);}));assert(oversized);
  // A different browser origin cannot open this gateway.
  const foreign=await browser.newPage();await foreign.goto('about:blank');const denied=await foreign.evaluate(address=>new Promise(resolve=>{const s=new WebSocket(`ws://${address}/websockify`);s.onopen=()=>{s.close();resolve(false)};s.onerror=()=>resolve(true);setTimeout(()=>resolve(false),3000);}),address);assert(denied);await foreign.close();
  await page.locator('#password').fill('fixture');await page.locator('#connect').click();await page.waitForFunction(()=>document.getElementById('status').textContent.startsWith('Connected'));
  const stalled=net.connect({host:'127.0.0.1',port});await new Promise((r,j)=>{stalled.once('connect',r);stalled.once('error',j);});stalled.write('GET / HTTP/1.1\r\n');const stalledClosed=new Promise(r=>{stalled.on('close',r);stalled.on('error',()=>{});});
  fs.writeFileSync(path.join(dir,'stop'),'stop');assert.equal(await exited,0,stdout);await Promise.race([stalledClosed,sleep(2000).then(()=>{throw Error('stalled request did not cancel')})]);assert(fs.existsSync(path.join(dir,'stopped')));await page.waitForFunction(()=>!document.getElementById('status').textContent.startsWith('Connected'));
  await browser.close();browser=undefined;fs.rmSync(profile,{recursive:true,force:true});assert(!fs.existsSync(profile));
  return {serverControl:control,canvasPixel:[19,111,173,255],dimensions:[64,48],checks:['password rejection','RFB authentication','actual WebSocket framebuffer','view-only input suppressed','server control policy','owned input events','disconnect/reconnect','same-origin','literal Host','unsupported/query/traversal paths','oversized headers','oversized WebSocket','no password URL/storage','offline local assets','CSP/cache','Stop closes RFB + HTTP + active viewer + stalled request','private profile removed'],moduleRequests:requests.length,events:events().trim().split('\n').filter(Boolean)};
 }finally {if(browser)await browser.close();if(test&&test.exitCode===null){fs.writeFileSync(path.join(dir,'stop'),'stop');await sleep(100);if(test.exitCode===null)test.kill('SIGTERM');}fs.rmSync(dir,{recursive:true,force:true});}
}
(async()=>{const evidence=[];for(const control of [false,true])evidence.push(await run(control));console.log(JSON.stringify({browser:'/usr/bin/chromium',qualification:evidence},null,2));})().catch(e=>{console.error(e);process.exitCode=1;});
