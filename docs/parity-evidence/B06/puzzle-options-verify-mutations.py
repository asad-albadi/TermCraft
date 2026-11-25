import os,subprocess,json
from pathlib import Path
root=Path('app/target/agent-work/b06-puzzle-options');base=root/'src/games/puzzles'
mutations=[
 ('solo-symmetry','solo.rs','SoloSymmetry::Rotate2 => vec![(x, y), (m - x, m - y)]','SoloSymmetry::Rotate2 => vec![(x, y)]','solo::options_tests::default_clues_have_half_turn_symmetry'),
 ('solo-block-grade','solo.rs','deduction(&digits, config, config.difficulty).is_none()','deduction(&digits, config, SoloDifficulty::Basic).is_none()','solo::options_tests::default_is_solved_using_only_block_position_elimination'),
 ('net-wrap','net.rs','else if c.wrapping','else if false','net::configuration_tests::wrapped_seam_connects_but_barrier_stops_it'),
 ('net-barrier','net.rs','&& self.barriers[i] & a == 0\n                    && self.barriers[j] & b == 0','&& true','net::configuration_tests::wrapped_seam_connects_but_barrier_stops_it'),
 ('net-loop','net.rs','if valid && tree && self.connected_at(0)','if valid && self.connected_at(0)','net::additional_configuration_tests::default_fallback_is_unique_and_loops_do_not_win'),
 ('redo-noop','mod.rs','if self.state == before {\n            return Ok(false);','if self.state == before {\n            self.future.clear();\n            return Ok(false);','tests::redo_restores_moves_and_only_changed_branch_clears_it'),
 ('solve-assisted','mod.rs','self.assisted = true;','self.assisted = false;','tests::explicit_solve_is_atomic_assisted_undoable_and_guess_gives_up'),
 ('guess-distinct','guess.rs','if !self.config.allow_multiple\n','if false\n','guess::configuration_tests::super_and_blank_distinct_rules'),
 ('guess-blanks','guess.rs','|| self.current.iter().all(|&c| c == 0)','|| false','guess::configuration_tests::super_and_blank_distinct_rules'),
 ('mines-corrections','mines.rs','self.solution_revealed = true;','self.solution_revealed = false;','tests::mines_explicit_solve_after_death_reveals_corrections_and_undo_hides_them'),
 ('untangle-points','untangle.rs','if !(4..=25).contains(&points)','if !(4..=10).contains(&points)','untangle::configuration_tests::stock_point_counts_have_valid_solved_embeddings'),
]
results=[]
for name,file,old,new,test in mutations:
 p=base/file;original=p.read_bytes();text=original.decode();assert old in text,(name,'anchor absent')
 try:
  p.write_text(text.replace(old,new,1))
  run=subprocess.run(['cargo','test','--manifest-path',str(root/'Cargo.toml'),test,'--','--nocapture'],env={**os.environ,'CARGO_BUILD_JOBS':'2'},capture_output=True,text=True)
  output=run.stdout+run.stderr;(root/f'mutation-{name}-red.log').write_text(output)
  result={'name':name,'file':file,'old':old,'new':new,'test':test,'exit':run.returncode,'behavioral_failure':'FAILED' in output and 'panicked' in output}
  results.append(result)
  assert run.returncode!=0 and result['behavioral_failure'],(name,output)
 finally:p.write_bytes(original)
(root/'mutation-results.json').write_text(json.dumps(results,indent=2))
print(f'{len(results)} behavioral mutation failures; all originals restored')
