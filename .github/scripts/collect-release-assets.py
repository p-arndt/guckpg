import argparse
import json
import shutil
from pathlib import Path


def collect(bundle: Path, output: Path, target: str, version: str) -> None:
    if target == "aarch64-apple-darwin":
        arch, suffixes = "aarch64", (".app.tar.gz", ".dmg")
    elif target == "x86_64-apple-darwin":
        arch, suffixes = "x64", (".app.tar.gz", ".dmg")
    elif target == "x86_64-pc-windows-msvc":
        arch, suffixes = "x64", ("-setup.exe", ".msi")
    else:
        raise ValueError(f"Unsupported target: {target}")

    output.mkdir(parents=True, exist_ok=True)
    for suffix in suffixes:
        matches = [path for path in bundle.rglob(f"*{suffix}") if path.is_file()]
        if len(matches) != 1:
            raise ValueError(f"Expected one {suffix} in {bundle}, found {len(matches)}")
        source = matches[0]
        name = f"guckpg_{version}_{arch}.app.tar.gz" if suffix == ".app.tar.gz" else source.name
        shutil.copy2(source, output / name)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("target")
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    version = json.loads(Path("package.json").read_text(encoding="utf-8"))["version"]
    collect(Path("target") / args.target / "release" / "bundle", args.output, args.target, version)
