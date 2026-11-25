use super::{Outcome, TerminalInput};

// Shared by the public scene, projectile origin and both collision paths.
const PLAYER_Y: i32 = 22;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Invader {
    pub x: i32,
    pub y: i32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NinvadersScene {
    pub width: i32,
    pub height: i32,
    pub player_x: i32,
    pub player_y: i32,
    pub enemies: Vec<Invader>,
    pub player_shots: Vec<(i32, i32)>,
    pub enemy_shots: Vec<(i32, i32)>,
    pub lives: u8,
    pub score: u32,
    pub started: bool,
    pub paused: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Invaders {
    pub player: i32,
    pub enemies: Vec<Invader>,
    pub shots: Vec<(i32, i32)>,
    pub enemy_shots: Vec<(i32, i32)>,
    pub lives: u8,
    pub score: u32,
    pub started: bool,
    pub paused: bool,
    pub ticks: u32,
    pub direction: i32,
    pub outcome: Outcome,
}
impl Invaders {
    pub fn new() -> Self {
        Self {
            player: 15,
            enemies: (0..3)
                .flat_map(|y| {
                    (0..8).map(move |x| Invader {
                        x: 4 + x * 3,
                        y: 2 + y * 2,
                    })
                })
                .collect(),
            shots: Vec::new(),
            enemy_shots: Vec::new(),
            lives: 3,
            score: 0,
            started: false,
            paused: false,
            ticks: 0,
            direction: 1,
            outcome: Outcome::Playing,
        }
    }
    pub fn scene(&self) -> NinvadersScene {
        NinvadersScene {
            width: 32,
            height: 24,
            player_x: self.player,
            player_y: PLAYER_Y,
            enemies: self.enemies.clone(),
            player_shots: self.shots.clone(),
            enemy_shots: self.enemy_shots.clone(),
            lives: self.lives,
            score: self.score,
            started: self.started,
            paused: self.paused,
        }
    }
    fn hits(&mut self) {
        self.shots.retain(|&(x, y)| {
            if let Some(i) = self.enemies.iter().position(|e| e.x == x && e.y == y) {
                self.enemies.remove(i);
                self.score = self.score.saturating_add(10);
                false
            } else {
                true
            }
        });
        self.enemy_shots.retain(|&(x, y)| {
            if x == self.player && y == PLAYER_Y {
                self.lives = self.lives.saturating_sub(1);
                false
            } else {
                true
            }
        });
        if self.lives == 0 || self.enemies.iter().any(|e| e.y >= PLAYER_Y) {
            self.outcome = Outcome::Lost
        } else if self.enemies.is_empty() {
            self.outcome = Outcome::Won
        }
    }
    pub fn apply(&mut self, i: TerminalInput) -> Result<(), &'static str> {
        if i == TerminalInput::Start {
            self.started = true;
            return Ok(());
        }
        if !self.started {
            return Err("Start the game first");
        }
        if i == TerminalInput::Pause {
            self.paused = !self.paused;
            return Ok(());
        }
        if self.paused {
            return Ok(());
        }
        match i {
            TerminalInput::Move { dx, dy } => {
                if !(-1..=1).contains(&dx) || dy != 0 {
                    return Err("Move left or right only");
                }
                self.player = (self.player + dx as i32).clamp(0, 31)
            }
            TerminalInput::Fire => {
                if self.shots.len() < 4
                    && !self
                        .shots
                        .iter()
                        .any(|&(x, y)| x == self.player && y == PLAYER_Y - 1)
                {
                    self.shots.push((self.player, PLAYER_Y - 1));
                }
            }
            TerminalInput::Tick => {
                self.ticks = self.ticks.wrapping_add(1);
                for p in &mut self.shots {
                    p.1 -= 1
                }
                for p in &mut self.enemy_shots {
                    p.1 += 1
                }
                self.hits();
                if self.outcome != Outcome::Playing {
                    return Ok(());
                }
                if self.ticks % 4 == 0 {
                    if self
                        .enemies
                        .iter()
                        .any(|e| e.x + self.direction < 0 || e.x + self.direction >= 32)
                    {
                        self.direction = -self.direction;
                        for e in &mut self.enemies {
                            e.y += 1
                        }
                    } else {
                        for e in &mut self.enemies {
                            e.x += self.direction
                        }
                    }
                    self.hits();
                }
                if self.ticks % 12 == 0 && self.enemy_shots.len() < 8 {
                    if let Some(e) = self
                        .enemies
                        .iter()
                        .max_by_key(|e| e.y * 100 - (e.x - self.player).abs())
                    {
                        self.enemy_shots.push((e.x, e.y + 1));
                    }
                }
                self.shots.retain(|&(_, y)| y >= 0);
                self.enemy_shots.retain(|&(_, y)| y < 24);
                self.hits();
            }
            _ => return Err("Expected movement, fire, tick or pause"),
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn player_shot_destroys_last_invader_and_wins() {
        let mut s = Invaders::new();
        s.started = true;
        s.enemies = vec![Invader { x: 15, y: 20 }];
        s.apply(TerminalInput::Fire).unwrap();
        s.apply(TerminalInput::Tick).unwrap();
        assert_eq!(s.outcome, Outcome::Won);
        assert_eq!(s.score, 10);
    }
    #[test]
    fn enemy_shot_costs_life_and_ground_contact_loses() {
        let mut s = Invaders::new();
        s.started = true;
        s.lives = 1;
        s.enemy_shots = vec![(15, 21)];
        s.apply(TerminalInput::Tick).unwrap();
        assert_eq!(s.outcome, Outcome::Lost);
        assert_eq!(s.lives, 0);
    }
}

#[cfg(test)]
mod player_row_contract {
    use super::*;
    #[test]
    fn public_player_row_matches_shot_origin_and_damage_boundary() {
        let mut s = Invaders::new();
        s.apply(TerminalInput::Start).unwrap();
        let row = s.scene().player_y;
        assert_eq!(row, 22);
        assert!(row >= 0 && row < s.scene().height);
        s.apply(TerminalInput::Fire).unwrap();
        assert_eq!(s.scene().player_shots, vec![(s.scene().player_x, row - 1)]);
        s.enemy_shots = vec![(s.scene().player_x, row - 2)];
        s.apply(TerminalInput::Tick).unwrap();
        assert_eq!(s.scene().lives, 3);
        s.apply(TerminalInput::Tick).unwrap();
        assert_eq!(s.scene().lives, 2);
        s.enemies = vec![Invader { x: 0, y: row }];
        s.apply(TerminalInput::Tick).unwrap();
        assert_eq!(s.outcome, Outcome::Lost);
    }
}
