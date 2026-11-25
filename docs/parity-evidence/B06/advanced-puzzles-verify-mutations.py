from pathlib import Path
import subprocess, shutil, json, os
root=Path(os.environ.get('B06_ADVANCED_ROOT',Path(__file__).resolve().parent)).resolve()
work=root/'work'
mutations=[
 ('solo-intersection','solo_advanced.rs','fn intersection(', 'intersections_remove_only_candidates_outside_overlap'),
 ('solo-set','solo_advanced.rs','fn set_eliminate(', 'full_set_rule_eliminates_naked_triple'),
 ('solo-fish','solo_advanced.rs','fn fish(', 'extreme_fish_eliminates_common_columns'),
 ('solo-forcing','solo_advanced.rs','fn forcing(', 'extreme_forcing_traverses_bivalue_peer_chain'),
 ('map-forcing','map.rs','fn forcing_chains(', 'forcing_chain_deduction_is_real'),
 ('killer-sum-table','solo_advanced.rs',('t[count][sum].push(bits);','/* mutation: remove valid sum combination */'),'killer_distinct_sum_table_matches_independent_enumeration'),
 ('jigsaw-regions','solo_advanced.rs',('regions[i] == regions[j]','(i / 27 == j / 27 && i % 9 / 3 == j % 9 / 3)'),'jigsaw_peer_conflicts_follow_irregular_regions_instead_of_boxes'),
 ('killer-45-residual','solo_advanced.rs',('if enable_intersections {','if false {'),'killer_stock_requires_source_intersection_grade'),
 ('killer-completion-sum','solo.rs',('(complete && sum != cage.sum)','(complete && false)'),'killer_wrong_sum_blocks_win_even_when_all_sudoku_units_are_valid'),
]
results=[]
for name,file,change,test in mutations:
 dest=root/'mutants'/name
 shutil.rmtree(dest,ignore_errors=True)
 shutil.copytree(work/'src',dest/'src')
 shutil.copy(work/'Cargo.toml',dest/'Cargo.toml')
 path=dest/'src/games/puzzles'/file;s=path.read_text()
 if isinstance(change,str):
  a=s.index(change);idx=s.index('{',a)+1;s=s[:idx]+'\n return 0; // mutation: suppress this source deduction\n'+s[idx:]
 else:
  old,new=change;assert old in s,(name,old);s=s.replace(old,new,1)
 path.write_text(s)
 env=dict(os.environ,CARGO_BUILD_JOBS='2',CARGO_TARGET_DIR=str(root/'mutation-target'))
 command=['cargo','test','--release','--manifest-path',str(dest/'Cargo.toml'),'-j','2',test,'--','--nocapture']
 result=subprocess.run(command,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,env=env)
 (root/f'mutation-{name}-red.log').write_text(result.stdout)
 caught=result.returncode!=0 and 'test result: FAILED' in result.stdout and 'panicked at' in result.stdout
 results.append({'mutation':name,'test':test,'exit':result.returncode,'caught_by_behavioral_assertion':caught})
 print(name,caught,flush=True)
 assert caught,(name,result.stdout)
(root/'mutation-results.json').write_text(json.dumps(results,indent=2)+'\n')
