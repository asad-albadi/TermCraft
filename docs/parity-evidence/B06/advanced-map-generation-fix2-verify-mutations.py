from pathlib import Path
import shutil,subprocess,json,os
r=Path(__file__).resolve().parent
cases=[('deterministic-colouring','generation_colouring_is_bounded_randomized_and_preserves_constraints'),('weaken-final-grade','actual_failing_ordinary_seeds_and_all_six_advanced_maps'),('unbounded-colouring','generation_colouring_is_bounded_randomized_and_preserves_constraints')]
results=[]
for name,test in cases:
 d=r/'mutants'/name;shutil.rmtree(d,ignore_errors=True);shutil.copytree(r/'work/src',d/'src');shutil.copy(r/'work/Cargo.toml',d/'Cargo.toml');p=d/'src/games/puzzles/map.rs';s=p.read_text()
 if name=='deterministic-colouring':
  a=s.index('fn random_colouring(');b=s.index('impl Map {',a)
  s=s[:a]+'''fn random_colouring(c:&mut[u8],edges:&[(usize,usize)],_rng:&mut Rng,budget:&mut usize)->bool {
    let mut first=None;search(c,edges,1,&mut first,budget);
    if let Some(answer)=first{c.copy_from_slice(&answer);true}else{false}
}
''' +s[b:]
 elif name=='weaken-final-grade':
  old='if certificate.achieved != config.difficulty || answer != solution {';assert old in s;s=s.replace(old,'if answer != solution {',1)
 else:
  a=s.index('fn random_colouring(');b=s.index('impl Map {',a);chunk=s[a:b];assert 'if *budget == 0 {' in chunk;chunk=chunk.replace('if *budget == 0 {','if false {',1);s=s[:a]+chunk+s[b:]
 p.write_text(s)
 env=dict(os.environ,CARGO_BUILD_JOBS='2',CARGO_TARGET_DIR=str(r/'mutation-target'))
 out=subprocess.run(['cargo','test','--release','--manifest-path',str(d/'Cargo.toml'),'-j','2',test,'--','--nocapture'],text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,env=env)
 (r/f'mutation-{name}-red.log').write_text(out.stdout)
 caught=out.returncode!=0 and 'test result: FAILED' in out.stdout and 'panicked at' in out.stdout
 results.append({'mutation':name,'test':test,'exit':out.returncode,'behaviorally_caught':caught});print(name,caught,flush=True);assert caught,out.stdout
(r/'mutation-results.json').write_text(json.dumps(results,indent=2)+'\n')
