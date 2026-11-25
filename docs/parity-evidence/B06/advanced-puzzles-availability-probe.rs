
#[test]
fn remaining_map_presets_have_configurations() {
 let entries:Vec<_>=PuzzleKind::Map.presets().into_iter().filter(|p|p.name.ends_with(", Hard")||p.name.ends_with(", Unreasonable")).collect();
 assert_eq!(entries.len(),6);
 for p in entries {assert!(p.configuration.is_some(),"{} is unavailable",p.name);}
}
#[test]
fn remaining_solo_presets_have_configurations() {
 let entries:Vec<_>=PuzzleKind::Solo.presets().into_iter().filter(|p|p.name.contains("Intermediate")||p.name.contains("Advanced")||p.name.contains("Extreme")||p.name.contains("Unreasonable")||p.name.contains("Killer")||p.name.contains("Jigsaw")).collect();
 assert_eq!(entries.len(),9);
 for p in entries {assert!(p.configuration.is_some(),"{} is unavailable",p.name);}
}
