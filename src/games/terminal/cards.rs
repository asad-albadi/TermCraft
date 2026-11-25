use super::{Outcome, Rng, TerminalInput};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolitaireVariant {
    Freecell,
    Klondike,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Card {
    pub rank: u8,
    pub suit: u8,
    pub face_up: bool,
}
impl Card {
    fn red(self) -> bool {
        self.suit == 1 || self.suit == 2
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardPile {
    Tableau(usize),
    Reserve(usize),
    Foundation(usize),
    Waste,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SolitermScene {
    pub variant: SolitaireVariant,
    pub tableau: Vec<Vec<Card>>,
    pub reserve: [Option<Card>; 4],
    pub foundations: [u8; 4],
    pub stock_count: usize,
    pub waste: Vec<Card>,
    pub passes_left: u8,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Cards {
    pub variant: SolitaireVariant,
    pub tableau: Vec<Vec<Card>>,
    pub reserve: [Option<Card>; 4],
    pub foundations: [u8; 4],
    pub stock: Vec<Card>,
    pub waste: Vec<Card>,
    pub passes_left: u8,
}
impl Cards {
    pub fn new(r: &mut Rng, variant: SolitaireVariant, passes: u8) -> Self {
        let mut deck: Vec<Card> = (0..4)
            .flat_map(|suit| {
                (1..=13).map(move |rank| Card {
                    rank,
                    suit,
                    face_up: true,
                })
            })
            .collect();
        r.shuffle(&mut deck);
        let mut tableau = vec![
            Vec::new();
            if variant == SolitaireVariant::Freecell {
                8
            } else {
                7
            }
        ];
        if variant == SolitaireVariant::Freecell {
            for (i, c) in deck.drain(..).enumerate() {
                tableau[i % 8].push(c)
            }
        } else {
            for (i, col) in tableau.iter_mut().enumerate() {
                for j in 0..=i {
                    let mut c = deck.pop().unwrap();
                    c.face_up = j == i;
                    col.push(c)
                }
            }
        }
        Self {
            variant,
            tableau,
            reserve: [None; 4],
            foundations: [0; 4],
            stock: deck,
            waste: Vec::new(),
            passes_left: passes,
        }
    }
    pub fn scene(&self) -> SolitermScene {
        SolitermScene {
            variant: self.variant,
            tableau: self.tableau.clone(),
            reserve: self.reserve,
            foundations: self.foundations,
            stock_count: self.stock.len(),
            waste: self.waste.clone(),
            passes_left: self.passes_left,
        }
    }
    pub fn outcome(&self) -> Outcome {
        if self.foundations == [13; 4] {
            Outcome::Won
        } else {
            Outcome::Playing
        }
    }
    pub fn apply(&mut self, i: TerminalInput) -> Result<(), &'static str> {
        if i == TerminalInput::Draw {
            if self.variant != SolitaireVariant::Klondike {
                return Err("Only Klondike has stock");
            }
            if let Some(mut c) = self.stock.pop() {
                c.face_up = true;
                self.waste.push(c);
                return Ok(());
            }
            if self.waste.is_empty() {
                return Ok(());
            }
            if self.passes_left <= 1 {
                return Err("No passes remain");
            }
            self.passes_left -= 1;
            self.stock = self.waste.drain(..).rev().collect();
            return Ok(());
        }
        let TerminalInput::Transfer { from, to, count } = i else {
            return Err("Expected card transfer or draw");
        };
        if from == to || count == 0 || count > 52 {
            return Err("Invalid transfer");
        }
        let moving: Vec<Card> = match from {
            CardPile::Tableau(n) => {
                let col = self.tableau.get(n).ok_or("Invalid source column")?;
                if count > col.len() {
                    return Err("Not enough cards");
                }
                col[col.len() - count..].to_vec()
            }
            CardPile::Reserve(n) => {
                if self.variant != SolitaireVariant::Freecell || count != 1 {
                    return Err("Invalid reserve move");
                }
                vec![
                    self.reserve
                        .get(n)
                        .copied()
                        .flatten()
                        .ok_or("Empty or invalid reserve")?,
                ]
            }
            CardPile::Waste => {
                if count != 1 {
                    return Err("Only top waste card can move");
                }
                vec![*self.waste.last().ok_or("Waste empty")?]
            }
            CardPile::Foundation(n) => {
                if n >= 4 || count != 1 || self.foundations[n] == 0 {
                    return Err("Invalid foundation source");
                }
                vec![Card {
                    rank: self.foundations[n],
                    suit: n as u8,
                    face_up: true,
                }]
            }
        };
        if moving.iter().any(|c| !c.face_up)
            || !moving
                .windows(2)
                .all(|p| p[0].rank == p[1].rank + 1 && p[0].red() != p[1].red())
        {
            return Err("Cards must form a face-up descending alternating run");
        }
        let first = moving[0];
        match to {
            CardPile::Tableau(n) => {
                let col = self.tableau.get(n).ok_or("Invalid destination column")?;
                if let Some(last) = col.last() {
                    if !last.face_up || last.rank != first.rank + 1 || last.red() == first.red() {
                        return Err("Tableau requires descending alternate colours");
                    }
                } else if self.variant == SolitaireVariant::Klondike && first.rank != 13 {
                    return Err("Only Kings fill empty Klondike columns");
                }
                if self.variant == SolitaireVariant::Freecell && count > 1 {
                    let empty_reserves = self.reserve.iter().filter(|c| c.is_none()).count();
                    let empty_columns = self
                        .tableau
                        .iter()
                        .enumerate()
                        .filter(|(j, c)| *j != n && c.is_empty())
                        .count();
                    let capacity = (empty_reserves + 1) * (1usize << empty_columns);
                    if count > capacity {
                        return Err("Insufficient free cells for sequence");
                    }
                }
            }
            CardPile::Reserve(n) => {
                if self.variant != SolitaireVariant::Freecell
                    || n >= 4
                    || count != 1
                    || self.reserve[n].is_some()
                {
                    return Err("Invalid reserve destination");
                }
            }
            CardPile::Foundation(n) => {
                if n >= 4
                    || count != 1
                    || first.suit as usize != n
                    || first.rank != self.foundations[n] + 1
                {
                    return Err("Foundations ascend in suit from Ace");
                }
            }
            CardPile::Waste => return Err("Cannot move cards to waste"),
        }
        match from {
            CardPile::Tableau(n) => {
                let len = self.tableau[n].len();
                self.tableau[n].truncate(len - count);
                if let Some(c) = self.tableau[n].last_mut() {
                    c.face_up = true
                }
            }
            CardPile::Reserve(n) => self.reserve[n] = None,
            CardPile::Waste => {
                self.waste.pop();
            }
            CardPile::Foundation(n) => self.foundations[n] -= 1,
        }
        match to {
            CardPile::Tableau(n) => self.tableau[n].extend(moving),
            CardPile::Reserve(n) => self.reserve[n] = Some(first),
            CardPile::Foundation(n) => self.foundations[n] += 1,
            CardPile::Waste => unreachable!(),
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn card(rank: u8, suit: u8) -> Card {
        Card {
            rank,
            suit,
            face_up: true,
        }
    }
    fn board(v: SolitaireVariant) -> Cards {
        Cards {
            variant: v,
            tableau: vec![vec![]; 8],
            reserve: [None; 4],
            foundations: [0; 4],
            stock: vec![],
            waste: vec![],
            passes_left: 2,
        }
    }
    #[test]
    fn freecell_foundations_ascending_suit_and_alternating_runs() {
        let mut s = board(SolitaireVariant::Freecell);
        s.tableau[0] = vec![card(1, 0)];
        s.apply(TerminalInput::Transfer {
            from: CardPile::Tableau(0),
            to: CardPile::Foundation(0),
            count: 1,
        })
        .unwrap();
        assert_eq!(s.foundations[0], 1);
        s.tableau[0] = vec![card(5, 0), card(4, 1)];
        s.tableau[1] = vec![card(6, 2)];
        s.apply(TerminalInput::Transfer {
            from: CardPile::Tableau(0),
            to: CardPile::Tableau(1),
            count: 2,
        })
        .unwrap();
        assert_eq!(s.tableau[1].len(), 3);
        s.tableau[2] = vec![card(7, 0)];
        assert!(
            s.apply(TerminalInput::Transfer {
                from: CardPile::Tableau(1),
                to: CardPile::Tableau(2),
                count: 2
            })
            .is_err()
        );
    }
    #[test]
    fn klondike_king_empty_and_turns_exposed_card() {
        let mut s = board(SolitaireVariant::Klondike);
        s.tableau.truncate(7);
        let mut hidden = card(6, 0);
        hidden.face_up = false;
        s.tableau[0] = vec![hidden, card(13, 1)];
        s.apply(TerminalInput::Transfer {
            from: CardPile::Tableau(0),
            to: CardPile::Tableau(1),
            count: 1,
        })
        .unwrap();
        assert!(s.tableau[0][0].face_up);
        assert!(
            s.apply(TerminalInput::Transfer {
                from: CardPile::Tableau(0),
                to: CardPile::Tableau(2),
                count: 1
            })
            .is_err()
        );
    }
    #[test]
    fn draw_and_recycle_respect_pass_limit() {
        let mut s = board(SolitaireVariant::Klondike);
        s.stock = vec![card(2, 0), card(3, 1)];
        s.passes_left = 1;
        s.apply(TerminalInput::Draw).unwrap();
        assert_eq!(s.waste.last().unwrap().rank, 3);
        s.apply(TerminalInput::Draw).unwrap();
        assert!(s.apply(TerminalInput::Draw).is_err());
    }
}

#[cfg(test)]
mod more_tests {
    use super::*;
    #[test]
    fn deals_conserve_every_card_and_have_correct_faceup_counts() {
        for v in [SolitaireVariant::Freecell, SolitaireVariant::Klondike] {
            for seed in 0..64 {
                let s = Cards::new(&mut Rng(seed), v, 3);
                let mut cards: Vec<_> = s
                    .tableau
                    .iter()
                    .flatten()
                    .chain(s.stock.iter())
                    .map(|c| (c.suit, c.rank))
                    .collect();
                cards.sort();
                assert_eq!(
                    cards,
                    (0..4)
                        .flat_map(|s| (1..=13).map(move |r| (s, r)))
                        .collect::<Vec<_>>()
                );
                let up = s.tableau.iter().flatten().filter(|c| c.face_up).count();
                assert_eq!(
                    up,
                    if v == SolitaireVariant::Freecell {
                        52
                    } else {
                        7
                    }
                );
            }
        }
    }
    #[test]
    fn reserve_capacity_blocks_sequence_without_empty_spaces() {
        let mut s = Cards::new(&mut Rng(1), SolitaireVariant::Freecell, 3);
        for c in &mut s.tableau {
            *c = vec![Card {
                rank: 9,
                suit: 0,
                face_up: true,
            }]
        }
        s.tableau[0] = vec![
            Card {
                rank: 5,
                suit: 0,
                face_up: true,
            },
            Card {
                rank: 4,
                suit: 1,
                face_up: true,
            },
        ];
        s.tableau[1] = vec![Card {
            rank: 6,
            suit: 1,
            face_up: true,
        }];
        s.reserve = [Some(Card {
            rank: 2,
            suit: 0,
            face_up: true,
        }); 4];
        let before = s.clone();
        assert!(
            s.apply(TerminalInput::Transfer {
                from: CardPile::Tableau(0),
                to: CardPile::Tableau(1),
                count: 2
            })
            .is_err()
        );
        assert_eq!(s, before);
    }
    #[test]
    fn foundation_requires_matching_suit_and_ace_then_two() {
        let mut s = Cards::new(&mut Rng(0), SolitaireVariant::Freecell, 3);
        s.tableau[0] = vec![Card {
            rank: 1,
            suit: 2,
            face_up: true,
        }];
        assert!(
            s.apply(TerminalInput::Transfer {
                from: CardPile::Tableau(0),
                to: CardPile::Foundation(0),
                count: 1
            })
            .is_err()
        );
        s.apply(TerminalInput::Transfer {
            from: CardPile::Tableau(0),
            to: CardPile::Foundation(2),
            count: 1,
        })
        .unwrap();
        assert_eq!(s.foundations[2], 1);
        s.foundations = [13; 4];
        assert_eq!(s.outcome(), Outcome::Won);
    }
}
