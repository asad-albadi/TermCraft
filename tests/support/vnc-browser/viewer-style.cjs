const { chromium } = require('playwright');
const { readFile } = require('node:fs/promises');
const path = require('node:path');
const assert = require('node:assert/strict');

// Serve project bytes inside an isolated browser context. No connection to a
// desktop is needed to measure the form controls or their accessibility states.
async function main() {
  const browser = await chromium.launch({executablePath: process.env.CHROMIUM || '/usr/bin/chromium'});
  try {
    for (const colorScheme of ['light', 'dark']) {
      const context = await browser.newContext({colorScheme, viewport: {width: 320, height: 480}});
      await context.route('http://viewer.test/**', async route => {
        const url = new URL(route.request().url());
        const local = url.pathname.startsWith('/novnc/')
          ? path.join('vendor/noVNC', url.pathname.slice(7))
          : path.join('assets/vnc-browser', url.pathname === '/' ? 'index.html' : url.pathname);
        const contentType = local.endsWith('.js') ? 'text/javascript' : local.endsWith('.css') ? 'text/css' : 'text/html';
        const body = await readFile(local).catch(() => undefined);
        await route.fulfill({status: body ? 200 : 404, contentType, body});
      });
      const page = await context.newPage();
      await page.goto('http://viewer.test/');
      const connect = page.getByRole('button', {name: 'Connect', exact: true});
      assert(await page.getByRole('button', {name:'Disconnect'}).isDisabled());
      assert(await page.getByRole('checkbox', {name:'View only'}).isChecked());
      assert.match(await page.getByRole('status').innerText(), /Disconnected/);
      await page.keyboard.press('Tab');
      assert.equal(await page.locator(':focus').getAttribute('type'), 'password');
      assert(await page.locator(':focus').evaluate(e => getComputedStyle(e).outlineStyle !== 'none'));
      await page.keyboard.press('Tab');
      await page.keyboard.press('Tab');
      assert.equal(await page.locator(':focus').getAttribute('id'), 'connect');
      const metrics = await connect.evaluate(e => {
        const rgb = color => color.match(/\d+/g).slice(0,3).map(x => Number(x)/255);
        const luma = color => rgb(color).map(x=>x<=0.04045?x/12.92:((x+.055)/1.055)**2.4)
          .reduce((sum,x,i)=>sum+x*[.2126,.7152,.0722][i],0);
        const style = getComputedStyle(e);
        const [low,high] = [luma(style.color),luma(style.backgroundColor)].sort((a,b)=>a-b);
        return {contrast:(high+.05)/(low+.05), overflow:document.documentElement.scrollWidth>innerWidth,
          fits:[...document.querySelectorAll('input,button')].every(el=>el.getBoundingClientRect().right<=innerWidth)};
      });
      assert(metrics.contrast >= 4.5 && !metrics.overflow && metrics.fits);
      await page.emulateMedia({reducedMotion:'reduce'});
      const background = () => connect.evaluate(e=>getComputedStyle(e).backgroundColor);
      assert.equal(await connect.evaluate(e=>getComputedStyle(e).transitionDuration),'0s');
      const rest = await background();
      await connect.hover();
      assert.notEqual(await background(),rest);
      const hover = await background();
      await page.mouse.down();
      assert.notEqual(await background(),hover);
      await page.mouse.up();
      await page.emulateMedia({forcedColors:'active'});
      await page.keyboard.press('Tab');
      assert(await page.locator(':focus').evaluate(e=>getComputedStyle(e).outlineStyle!=='none'));
      console.log(colorScheme,metrics);
      await context.close();
    }
  } finally { await browser.close(); }
}
main().catch(error => { console.error(error); process.exitCode = 1; });
