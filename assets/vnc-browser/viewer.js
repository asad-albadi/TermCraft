import RFB from '/novnc/core/rfb.js';
const $ = id => document.getElementById(id);
let rfb;
function status(text) { $('status').textContent = text; }
function finish() { rfb = undefined; $('connect').disabled = false; $('disconnect').disabled = true; $('password').value = ''; }
$('login').addEventListener('submit', event => {
    event.preventDefault();
    if (rfb) return;
    const password = $('password').value;
    $('password').value = '';
    if (!/^[\x20-\x7e]{1,8}$/.test(password)) { status('Enter 1–8 printable ASCII password bytes.'); return; }
    status('Connecting');
    const viewer = new RFB($('screen'), `ws://${location.host}/websockify`, { credentials: { password } });
    rfb = viewer;
    viewer.viewOnly = $('viewOnly').checked;
    viewer.scaleViewport = true;
    viewer.background = '#202825';
    $('connect').disabled = true; $('disconnect').disabled = false;
    viewer.addEventListener('connect', () => { status(viewer.viewOnly ? 'Connected (view only)' : 'Connected (server controls access)'); });
    viewer.addEventListener('disconnect', event => { status(event.detail.clean ? 'Disconnected' : 'Connection ended; check password and server access.'); finish(); });
    viewer.addEventListener('securityfailure', () => { status('Authentication failed.'); });
    viewer.addEventListener('credentialsrequired', () => { viewer.disconnect(); status('Authentication requires reconnecting with a password.'); });
});
$('disconnect').addEventListener('click', () => rfb?.disconnect());
$('viewOnly').addEventListener('change', () => { if (rfb) { rfb.viewOnly = $('viewOnly').checked; status(rfb.viewOnly ? 'Connected (view only)' : 'Connected (server controls access)'); } });
window.addEventListener('pagehide', () => { rfb?.disconnect(); $('password').value = ''; });
