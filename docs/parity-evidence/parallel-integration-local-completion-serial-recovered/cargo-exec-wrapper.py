#!/usr/bin/python3
import os,sys,json,time
from pathlib import Path
real='/home/asad/Documents/Craft Anything/mobaxterm-personal-26-6/app/target/rustup-validation-1961/toolchains/1.96.1-x86_64-unknown-linux-gnu/bin/cargo'
log=Path('/home/asad/Documents/Craft Anything/mobaxterm-personal-26-6/app/target/root-validation/serial-release-cargo/invocations.jsonl')
assert os.environ.get("CARGO_BUILD_JOBS")=="2"
args=sys.argv[1:]
if args and args[0] in ("build","check","test"):
    assert not any(a in ("-j","--jobs") or a.startswith("--jobs=") for a in args)
    args=[args[0],"--jobs","1",*args[1:]]
with log.open("a") as f:f.write(json.dumps({"time":time.time(),"command":[real,*args],"CARGO_BUILD_JOBS":os.environ["CARGO_BUILD_JOBS"]})+"\n")
os.execv(real,[real,*args])
