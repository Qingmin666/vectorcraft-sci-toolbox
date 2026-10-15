"""Package a verified native candidate, source overlays and dependency license notices."""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import zipfile


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def copy_tree(source: Path, destination: Path):
    excluded={'.git','.runtime','.dashboard','.venv','__pycache__'}
    for item in source.rglob('*'):
        relative=item.relative_to(source)
        if any(part in excluded for part in relative.parts) or item.suffix in ('.pyc','.pyo') or item.name.startswith('.env'):
            continue
        if item.is_file() and not item.is_symlink() and not any(parent.is_symlink() for parent in item.parents if parent==source or parent.is_relative_to(source)):
            target=destination/item.relative_to(source);target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(item,target)


def main():
    parser=argparse.ArgumentParser()
    for option in ('exe','upstream','metadata','cargo-home','validation'):
        parser.add_argument('--'+option,type=Path,required=True)
    parser.add_argument('--output',type=Path)
    parser.add_argument('--license-cache',type=Path)
    args=parser.parse_args();project=Path(__file__).resolve().parents[1]
    identity=json.loads((project/'native/upstream.json').read_text(encoding='utf-8'))
    output=(args.output or project/'dist/native').resolve();package=output/('v'+identity['editionVersion']);package.mkdir(parents=True,exist_ok=True)
    target_exe=package/'VectorCraftSCI.exe'
    if not target_exe.is_file() or sha256(args.exe)!=sha256(target_exe):shutil.copy2(args.exe,target_exe)
    shutil.copy2(project/'native/USAGE.zh-CN.txt',package/'使用说明.txt')
    licenses=package/'licenses';licenses.mkdir(exist_ok=True)
    shutil.copy2(project/'LICENSE',licenses/'LICENSE-SCI.txt')
    for name in ('LICENSE-MIT','LICENSE-APACHE','NOTICE','ASSETS.md'):
        shutil.copy2(args.upstream/name,licenses/name)
    copy_tree(project/'native/licenses',licenses/'toolchain-runtime')
    for asset in (args.upstream/'assets').rglob('*'):
        if asset.is_file() and re.match(r'(?i)^(license|licence|ofl-|copying|notice)',asset.name):
            target=licenses/'assets'/asset.relative_to(args.upstream/'assets');target.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(asset,target)
    brand=args.upstream/'docs/brand/LICENSE-brand.txt'
    if brand.is_file():shutil.copy2(brand,licenses/brand.name)
    metadata=json.loads(args.metadata.read_text(encoding='utf-8-sig'));packages={p['id']:p for p in metadata['packages']};nodes={n['id']:n for n in metadata['resolve']['nodes']}
    root=next(p['id'] for p in packages.values() if p['name']=='vectorcraft')
    pending=[root];seen=set()
    while pending:
        current=pending.pop()
        if current in seen:continue
        seen.add(current)
        for dep in nodes[current]['deps']:
            if any(kind.get('kind') in (None,'normal') for kind in dep['dep_kinds']):pending.append(dep['pkg'])
    allowed=[args.upstream.resolve(),args.cargo_home.resolve()];records=[];missing=[]
    cached=json.loads((args.license_cache/'index.json').read_text()) if args.license_cache else {}
    for package_id in sorted(seen):
        p=packages[package_id];folder=Path(p['manifest_path']).parent.resolve();found=[]
        for ancestor in [folder,*list(folder.parents)[:4]]:
            if not any(ancestor.is_relative_to(base) for base in allowed):continue
            found=[f for f in ancestor.iterdir() if f.is_file() and not f.is_symlink() and re.match(r'(?i)^(license|licence|copying|copyright|notice)',f.name)]
            if found:break
        key=re.sub(r'[^A-Za-z0-9._+-]','_',p['name']+'-'+p['version']);dest=licenses/'rust'/key;copied=[];provenance=[]
        if not found and key in cached:
            entry=cached[key];vcs=json.loads((folder/'.cargo_vcs_info.json').read_text())
            if vcs['git']['sha1']!=entry['sha1']:raise RuntimeError('License source commit mismatch: '+key)
            for item in entry['files']:
                item_path=(args.license_cache/item['file']).resolve()
                if not item_path.is_relative_to(args.license_cache.resolve()):raise RuntimeError('License cache path escapes its root')
                found.append(item_path);provenance.append(item['url'])
        for item in found:
            if item.stat().st_size>2_000_000:continue
            dest.mkdir(parents=True,exist_ok=True);target=dest/item.name;shutil.copy2(item,target);copied.append(str(target.relative_to(package)).replace('\\','/'))
        if not copied:missing.append(key)
        records.append({'name':p['name'],'version':p['version'],'license':p.get('license'),'files':copied,'sourceLicenseUrls':provenance})
    (licenses/'RUST-DEPENDENCIES.json').write_text(json.dumps({'packages':records,'missingLicenseFiles':missing},ensure_ascii=False,indent=2),encoding='utf-8')
    if missing:raise RuntimeError('License files missing: '+', '.join(missing))
    examples=package/'examples';examples.mkdir(exist_ok=True)
    for name in ('native-scientific-full.vectorcraft','native-scientific-full.png','native-rc2-combined.vectorcraft','native-rc2-combined.png'):
        if not (args.validation/name).is_file():continue
        shutil.copy2(args.validation/name,examples/name)
    for name in ('report.json','full-report.json','complete-report.json','palette-report.json','gui-report.json'):
        if not (args.validation/name).is_file():continue
        shutil.copy2(args.validation/name,package/('VALIDATION-'+name))
    copy_tree(project/'native',package/'source/native');copy_tree(project/'src',package/'source/src');copy_tree(project/'docs/native',package/'source/docs/native');shutil.copy2(project/'LICENSE',package/'source/LICENSE');shutil.copy2(project/'THIRD_PARTY_NOTICES.md',package/'THIRD_PARTY_NOTICES.md')
    scripts=package/'source/scripts';scripts.mkdir(parents=True,exist_ok=True)
    for name in ('build_native_windows.ps1','package_native_windows.py','fetch_native_license_cache.py','smoke_native_desktop.py','smoke_native_full.py','smoke_native_complete.py','smoke_native_palette.py','smoke_desktop.py'):
        shutil.copy2(project/'scripts'/name,scripts/name)
    manifest={'edition':identity['editionVersion'],'upstream':identity,'binarySha256':sha256(package/'VectorCraftSCI.exe'),'hostPatchSha256':sha256(project/'native/patches/host-integration.patch'),'rustDependencyPackages':len(records),'runtime':'independent native application; no Python service required'}
    (package/'BUILD.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2),encoding='utf-8')
    archive=output/f"vectorcraft-sci-native-v{identity['editionVersion']}-windows-x64.zip"
    with zipfile.ZipFile(archive,'w',compression=zipfile.ZIP_DEFLATED,compresslevel=6,strict_timestamps=False) as z:
        for item in sorted(package.rglob('*')):
            if item.is_file():z.write(item,item.relative_to(package).as_posix())
    sums=f"{sha256(package/'VectorCraftSCI.exe')}  VectorCraftSCI.exe\n{sha256(archive)}  {archive.name}\n"
    (output/f"SHA256SUMS-v{identity['editionVersion']}.txt").write_text(sums,encoding='utf-8')
    print(json.dumps({'package':str(package),'archive':str(archive),'rustDependencyPackages':len(records),'missingLicenseFiles':missing,'zipBytes':archive.stat().st_size},ensure_ascii=True))


if __name__=='__main__':main()
