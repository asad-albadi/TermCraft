"""Build AppImage library material from exact, signed-index Ubuntu packages."""
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import re
import subprocess
import tarfile

HERE = Path(__file__).resolve().parent
HOST = {"libc.so.6", "libm.so.6", "libpthread.so.0", "libdl.so.2", "librt.so.1", "ld-linux-x86-64.so.2"}
SEEDS = {"libgcc_s.so.1", "libX11.so.6", "libX11-xcb.so.1", "libxcb.so.1", "libXcursor.so.1",
         "libXi.so.6", "libXrandr.so.2", "libwayland-client.so.0", "libwayland-cursor.so.0",
         "libwayland-egl.so.1", "libxkbcommon.so.0", "libxkbcommon-x11.so.0"}


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def elf(path):
    with path.open("rb") as binary:
        header = binary.read(64)
    require(header[:7] == b"\x7fELF\x02\x01\x01" and header[18:20] == b">\x00", "Expected x86_64 ELF: " + path.name)
    output = subprocess.check_output(["readelf", "--wide", "--dynamic", path], text=True, env={"LC_ALL": "C"})
    require("(RPATH)" not in output and "(RUNPATH)" not in output, "Unexpected private loader path")
    needed = re.findall(r"\(NEEDED\).*?\[(.*?)\]", output)
    require(all(re.fullmatch(r"[\w.+-]+", soname) for soname in needed), "Invalid ELF import")
    symbols = subprocess.check_output(["readelf", "--wide", "--version-info", path], text=True, env={"LC_ALL": "C"})
    # Only verneed names are requirements. Version definitions name exported ABI.
    needed_symbols = symbols.partition("Version needs section")[2]
    versions = re.findall(r"\bName: GLIBC_(\d+(?:\.\d+)+)\b", needed_symbols)
    return {"needed": sorted(needed), "soname": re.findall(r"\(SONAME\).*?\[(.*?)\]", output),
            "glibc": max(versions, key=version_tuple, default=None)}


def version_tuple(value):
    return tuple(map(int, value.split(".")))


def deb_data(path):
    members = subprocess.check_output(["ar", "t", path], text=True).splitlines()
    payload, = [name for name in members if name.startswith("data.tar.")]
    data = subprocess.check_output(["ar", "p", path, payload])
    if payload.endswith(".zst"):
        data = subprocess.run(["zstd", "-dc"], input=data, stdout=subprocess.PIPE, check=True).stdout
    return tarfile.open(fileobj=io.BytesIO(data), mode="r:*")


def package_bytes(bundle, member):
    named = {PurePosixPath(m.name): m for m in bundle}
    current = PurePosixPath(member)
    for _ in range(6):
        item = named[current]
        if item.isfile():
            return bundle.extractfile(item).read()
        require(item.issym() and "/" not in item.linkname and item.linkname not in {".", ".."}, "Invalid package link")
        current = current.with_name(item.linkname)
    raise ValueError("Invalid package link chain")


def libraries(inputs, work):
    lock = json.loads((HERE / "ubuntu-lock.json").read_text())
    library = work / "libraries"
    notices = work / "notices"
    library.mkdir(parents=True)
    notices.mkdir()
    inventory = {}
    for package, info in lock["packages"].items():
        deb = inputs / Path(info["path"]).name
        require(digest(deb) == info["sha256"], "Incorrect pinned package: " + package)
        with deb_data(deb) as bundle:
            if package == "base-files":
                for item in bundle:
                    if item.isfile() and item.name.startswith("./usr/share/common-licenses/"):
                        (notices / ("common-" + Path(item.name).name)).write_bytes(bundle.extractfile(item).read())
                continue
            if package != "libgcc-s1":
                (notices / (package + ".copyright")).write_bytes(package_bytes(bundle, f"usr/share/doc/{package}/copyright"))
            soname = info["soname"]
            if not soname:
                continue
            path = library / soname
            path.write_bytes(package_bytes(bundle, "usr/lib/x86_64-linux-gnu/" + soname))
            observed = elf(path)
            require(observed["soname"] == [soname], "Wrong ELF SONAME: " + soname)
            inventory[soname] = {**observed, "sha256": digest(path), "package": package,
                                 "binary_version": info["version"], "source_package": info["source"],
                                 "source_version": lock["sources"][info["source"]]["version"]}
    needed = set(SEEDS)
    observed = set()
    while needed - observed:
        soname = (needed - observed).pop()
        require(soname in inventory, "Unclassified ELF dependency: " + soname)
        needed.update(set(inventory[soname]["needed"]) - HOST)
        observed.add(soname)
    require(observed == set(inventory), "Unexpected ELF payload beyond the display-client closure")
    return {"libraries": inventory, "notices": {p.name: digest(p) for p in sorted(notices.iterdir())}}
