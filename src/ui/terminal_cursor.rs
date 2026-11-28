//! Per-terminal cursor phase; input and focus changes restart the visible half-cycle.
use super::*;

#[derive(Clone, Copy)]
struct Blink {
    since: f64,
    position: (usize, usize),
    focused: bool,
}

pub(super) fn visible(ui: &Ui, tab: &TerminalTab, preferences: &Preferences) -> bool {
    if !tab.screen.cursor_visible || !tab.running() {
        return false;
    }
    let ctx = ui.ctx();
    let focused = ui.memory(|m| m.has_focus(Id::new(("terminal-input", tab.id))))
        && ui.input(|i| i.focused)
        && ui.memory(|m| m.allows_interaction(ui.layer_id()))
        && !super::shortcuts::input_blocked(ctx, tab.id);
    let now = ui.input(|i| i.time);
    let position = (tab.screen.row, tab.screen.col);
    let activity = focused
        && ui.input(|i| {
            i.events.iter().any(|e| {
                matches!(
                    e,
                    Event::Text(_) | Event::Paste(_) | Event::Key { pressed: true, .. }
                )
            })
        });
    let state = ctx.data_mut(|data| {
        let state =
            data.get_temp_mut_or_insert_with(Id::new(("terminal-cursor-blink", tab.id)), || {
                Blink {
                    since: now,
                    position,
                    focused,
                }
            });
        if state.focused != focused || state.position != position || activity || now < state.since {
            *state = Blink {
                since: now,
                position,
                focused,
            };
        }
        *state
    });
    if !focused || preferences.reduced_motion {
        return true;
    }
    const HALF_CYCLE: f64 = 0.6;
    let elapsed = now - state.since;
    let phase = elapsed % (2. * HALF_CYCLE);
    let next = if phase < HALF_CYCLE {
        HALF_CYCLE - phase
    } else {
        2. * HALF_CYCLE - phase
    };
    ctx.request_repaint_after(Duration::from_secs_f64(next.max(0.001)));
    phase < HALF_CYCLE
}
