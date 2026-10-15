"""Assemble the portable Windows artifact with notices and checksums."""
import argparse
import hashlib
import importlib.metadata
import json
from pathlib import Path
import sys
import zipfile

root=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser();parser.add_argument('--runtime-prefix',type=Path,default=Path(sys.base_prefix))
args=parser.parse_args()
destination=root/'dist'/'vectorcraft-sci-toolbox-v0.1.0-windows-x64.zip'
executable=root/'dist'/'VectorCraftSciToolbox.exe'
if not executable.is_file():raise SystemExit('Build executable first')
notices={}
python_license=args.runtime_prefix/'LICENSE_PYTHON.txt'
if python_license.exists():notices['notices/Python-LICENSE.txt']=python_license.read_bytes()
for package in ['pyinstaller','altgraph','packaging','pefile','pywin32-ctypes','setuptools','pyinstaller-hooks-contrib']:
    try:dist=importlib.metadata.distribution(package)
    except importlib.metadata.PackageNotFoundError:continue
    for f in dist.files or []:
        if any(word in f.name.lower() for word in ['license','copying','copyright']):
            path=Path(dist.locate_file(f))
            if path.is_file():notices[f'notices/{package}/{f.name}']=path.read_bytes()
# Retain cached license texts for the conda runtime's native libraries when available.
runtime_packages=[]
names={'python','openssl','zlib','libzlib','zstd','xz','liblzma','bzip2','libffi','mpdecimal','vc14_runtime','vs2015_runtime','ucrt'}
for metadata in (args.runtime_prefix/'conda-meta').glob('*.json'):
    data=json.loads(metadata.read_text(encoding='utf-8'))
    if data.get('name') not in names:continue
    runtime_packages.append({k:data.get(k) for k in ('name','version','license')})
    source=Path(data.get('link',{}).get('source',''))/'info'/'licenses'
    if source.is_dir():
        for path in source.rglob('*'):
            if path.is_file():notices[f"notices/runtime-{data['name']}/{path.relative_to(source).as_posix()}"]=path.read_bytes()
notices['notices/runtime-components.json']=json.dumps(runtime_packages,indent=2).encode()
files=[executable,root/'README.md',root/'LICENSE',root/'THIRD_PARTY_NOTICES.md',root/'CHANGELOG.md']
files+=list((root/'examples').glob('*'))
files+=list((root/'docs').glob('*'))
with zipfile.ZipFile(destination,'w',zipfile.ZIP_DEFLATED) as archive:
    for path in files:
        if path.is_file():archive.write(path,path.name if path.parent.name=='dist' else path.relative_to(root).as_posix())
    for name,content in notices.items():archive.writestr(name,content)
checksums=''.join(f'{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n' for p in [executable,destination])
(root/'dist'/'SHA256SUMS.txt').write_text(checksums,encoding='utf-8')
print(destination)
print('Retained notice files:',len(notices))
