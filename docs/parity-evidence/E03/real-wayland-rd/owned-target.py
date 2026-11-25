import gi,sys,json,time
from pathlib import Path
gi.require_version('Gtk','4.0');from gi.repository import Gtk,Gdk,GLib
out=Path(sys.argv[1]);out.mkdir(exist_ok=True)
def event(kind,**args):
 with (out/'events.jsonl').open('a') as f:f.write(json.dumps({'kind':kind,'time':time.monotonic(),**args})+'\n')
class App(Gtk.Application):
 def do_activate(self):
  w=Gtk.ApplicationWindow(application=self,title='OWNED NATIVE REMOTE DESKTOP TARGET');w.set_default_size(760,540)
  area=Gtk.DrawingArea();area.set_focusable(True)
  def draw(a,cr,width,height):
   cr.set_source_rgb(.08,.19,.31);cr.paint();cr.set_source_rgb(.15,.82,.57);cr.rectangle(40,100,width-80,height-140);cr.fill();cr.set_source_rgb(1,1,1);cr.set_font_size(26);cr.move_to(30,45);cr.show_text('OWNED WAYLAND RD TARGET');cr.set_font_size(18);cr.move_to(30,75);cr.show_text('Real native keyboard and pointer event receiver')
  area.set_draw_func(draw);w.set_child(area)
  key=Gtk.EventControllerKey();key.set_propagation_phase(Gtk.PropagationPhase.CAPTURE);key.connect('key-pressed',lambda c,k,code,state:event('key-down',keyval=k,keycode=code,state=int(state)) or False);key.connect('key-released',lambda c,k,code,state:event('key-up',keyval=k,keycode=code,state=int(state)));w.add_controller(key)
  click=Gtk.GestureClick();click.set_button(0);click.connect('pressed',lambda c,n,x,y:(area.grab_focus(),event('button-down',button=c.get_current_button(),x=x,y=y)));click.connect('released',lambda c,n,x,y:event('button-up',button=c.get_current_button(),x=x,y=y));area.add_controller(click)
  motion=Gtk.EventControllerMotion();motion.connect('motion',lambda c,x,y:event('motion',x=x,y=y));area.add_controller(motion)
  w.present();area.grab_focus();event('mapped')
app=App(application_id='org.termcraft.PrivateNativeRDTarget');app.run([])
