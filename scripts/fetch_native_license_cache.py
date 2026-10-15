"""Fetch missing crate license notices from recorded GitHub/Codeberg source commits."""
from __future__ import annotations
import argparse
from concurrent.futures import ThreadPoolExecutor
import json
from pathlib import Path
import re
from urllib.error import HTTPError
from urllib.parse import urlsplit
from urllib.request import Request,urlopen


def main():
    p=argparse.ArgumentParser();p.add_argument('--metadata',type=Path,required=True);p.add_argument('--missing',type=Path,required=True);p.add_argument('--cache',type=Path,required=True);args=p.parse_args()
    metadata=json.loads(args.metadata.read_text(encoding='utf-8-sig'));missing=set(json.loads(args.missing.read_text(encoding='utf-8'))['missingLicenseFiles']);cache=args.cache.resolve();cache.mkdir(parents=True,exist_ok=True)
    jobs={};packages={}
    for item in metadata['packages']:
        key=re.sub(r'[^A-Za-z0-9._+-]','_',item['name']+'-'+item['version'])
        if key not in missing:continue
        folder=Path(item['manifest_path']).parent;vcs=json.loads((folder/'.cargo_vcs_info.json').read_text());sha=vcs['git']['sha1'];url=urlsplit(item['repository']);parts=url.path.strip('/').split('/')
        if url.hostname not in ('github.com','codeberg.org') or len(parts)<2 or not re.fullmatch(r'[0-9a-f]{40}',sha):raise ValueError('Only recorded GitHub/Codeberg source commits are supported')
        repo=url.hostname+'/'+'/'.join(parts[:2]).removesuffix('.git');group=(repo,sha);packages[key]=group;jobs.setdefault(group,vcs.get('path_in_vcs',''))
    def fetch(job):
        (repo,sha),subdir=job;destination=cache/(repo.replace('/','--')+'--'+sha);destination.mkdir(exist_ok=True);results=[]
        names=['LICENSE_APACHE','LICENSE_MIT','LICENSES/Apache-2.0.txt','LICENSES/MIT.txt','LICENSE-MIT','LICENSE-APACHE','LICENSE','LICENSE.md','LICENSE.txt','COPYING','LICENSE_1_0.txt','LICENSE-APACHE-LLVM','LICENSE-Apache-2.0_WITH_LLVM-exception']
        prefixes=['']
        if subdir:prefixes += [subdir.strip('/')+'/']
        for prefix in prefixes:
            for name in names:
                host,repository=repo.split('/',1)
                if host=='codeberg.org' and repository=='xfix/enum-map':repository='sugar700/enum-map'
                url=f'https://raw.githubusercontent.com/{repository}/{sha}/{prefix}{name}' if host=='github.com' else f'https://codeberg.org/{repository}/raw/commit/{sha}/{prefix}{name}'
                try:
                    with urlopen(Request(url,headers={'User-Agent':'VectorCraftSCI-license-collector'}),timeout=15) as response:data=response.read(2_000_001)
                    if not data or len(data)>2_000_000 or data.lstrip().startswith(b'<'):continue
                    data.decode('utf-8');target=destination/((prefix.replace('/','_') if prefix else '')+name.replace('/','_'));target.write_bytes(data);results.append({'file':str(target.relative_to(cache)).replace('\\','/'),'url':url})
                except HTTPError as error:
                    if error.code!=404:raise
            if results:break
        return (repo,sha),results
    with ThreadPoolExecutor(max_workers=6) as executor:found=dict(executor.map(fetch,jobs.items()))
    index={key:{'repository':repo,'sha1':sha,'files':found[(repo,sha)]} for key,(repo,sha) in packages.items()}
    (cache/'index.json').write_text(json.dumps(index,indent=2),encoding='utf-8');unresolved=[key for key,value in index.items() if not value['files']]
    print(json.dumps({'packages':len(index),'sourceCommits':len(jobs),'unresolved':unresolved}))
    if unresolved:raise SystemExit(1)


if __name__=='__main__':main()
