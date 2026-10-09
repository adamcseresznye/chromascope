"""Local CLI library workflow; never overwrites evidence or downloads data.
Usage: python examples/spectral_workflow.py --cli target/debug/chromascope-cli.exe
  --library library.msp --format msp --source source.json --query inspected.json
  --config search.json --output new-evidence-directory
Query is a saved engine processing response (or GUI history containing one).
Source declares name/version/url/license; config uses the engine SearchConfig schema.
"""
import argparse,json,subprocess,uuid
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__)
for field in ['cli','library','format','source','query','config','output']:
    p.add_argument('--'+field,required=True)
a=p.parse_args()
out=Path(a.output)
out.mkdir(exist_ok=False)
def run(name,operation):
    request={'version':1,'operation_id':str(uuid.uuid4()),'actor':'cli-spectral-workflow','operation':operation}
    with (out/(name+'-request.json')).open('x',encoding='utf-8') as f:json.dump(request,f,indent=2)
    result=subprocess.run([a.cli,'run','-'],input=json.dumps(request).encode(),stdout=subprocess.PIPE,stderr=subprocess.PIPE,check=False)
    with (out/(name+'-response.json')).open('xb') as f:f.write(result.stdout)
    if result.returncode:raise RuntimeError(result.stdout.decode()+result.stderr.decode())
    return json.loads(result.stdout)['result']['output']
source=json.loads(Path(a.source).read_text(encoding='utf-8'))
library=run('library',{'operation':'import_spectral_library','text':Path(a.library).read_bytes().decode('utf-8'),'format':a.format,'source':source})['library']
query=json.loads(Path(a.query).read_text(encoding='utf-8'))
if isinstance(query,list):
    query=next(r for r in reversed(query) if r['output']['kind']=='spectral_processing')
query=query.get('result',query)['output']['processed']
config=json.loads(Path(a.config).read_text(encoding='utf-8'))
search=run('search',{'operation':'search_spectral_library','query':query,'library':library,'config':config})
print(f"Retained {len(search['report']['candidates'])} candidates; current identity remains unknown. Evidence: {out.resolve()}")
