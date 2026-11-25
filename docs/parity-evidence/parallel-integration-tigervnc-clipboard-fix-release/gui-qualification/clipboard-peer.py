import gi,sys,json
from pathlib import Path
gi.require_version('Gtk','4.0');from gi.repository import Gtk,Gdk
R=Path(sys.argv[1]);R.mkdir(exist_ok=True)
class Peer(Gtk.Application):
 def do_activate(self):
  w=Gtk.ApplicationWindow(application=self,title='OWNED LOCAL CLIPBOARD PEER');w.set_default_size(520,260)
  box=Gtk.Box(orientation=Gtk.Orientation.VERTICAL,spacing=16);box.set_margin_top(24);box.set_margin_start(24);box.set_margin_end(24);box.set_margin_bottom(24);w.set_child(box)
  label=Gtk.Label(label='Only this private compositor clipboard is observed.');box.append(label)
  read=Gtk.Button(label='Read owned clipboard');setb=Gtk.Button(label='Set CLIENT_CLIPBOARD_OWNED_GUI');box.append(read);box.append(setb)
  def read_clip(button):
   def done(cb,result,*_):
    try:
     text=cb.read_text_finish(result);(R/'read-result.json').write_text(json.dumps({'text':text}));label.set_text(text or 'empty')
    except Exception as e:(R/'read-result.json').write_text(json.dumps({'error':str(e)}))
   w.get_clipboard().read_text_async(None,done)
  def set_clip(button):w.get_clipboard().set('CLIENT_CLIPBOARD_OWNED_GUI');label.set_text('CLIENT_CLIPBOARD_OWNED_GUI');(R/'set-result.json').write_text(json.dumps({'marker_set':True}))
  read.connect('clicked',read_clip);setb.connect('clicked',set_clip);w.present()
Peer(application_id='org.termcraft.OwnedClipboardPeer').run([])
