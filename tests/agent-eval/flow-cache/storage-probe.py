import json,tempfile,time
from pathlib import Path
from corpus import sources,install,CASES
from fr_ir.context import DirectoryObjectStore,store_merkle_value,restore_stored_value
from fr_ir.runtime import FrClient
fr=Path('target/debug/fr').resolve(); rows=[]
with tempfile.TemporaryDirectory() as tmp:
 for name in CASES:
  root=Path(tmp)/name; install(root,sources(name)); client=FrClient(root,executable=fr,max_output_bytes=1048576)
  target=client.call('--no-cache','project','find','entry').definition_target().handle
  report=client.call('--no-cache','project','dataflow',target,'--summaries','--imports','--steps','4096','--depth','32','--bytes','1048576','--rules',str(root/'rules.json')).to_data()
  encoded=json.dumps(report,sort_keys=True,ensure_ascii=False,separators=(',',':'))
  row={'case':name,'complete':report['complete'],'cutoffs':report['cutoffs'],'bytes':len(encoded.encode()),'steps':report['execution']['analysis_steps']}
  for mode,value in [('tree',report),('chunks',{'schema':'fr-flow-record-1','chunks':[encoded[i:i+32768] for i in range(0,len(encoded),32768)]})]:
   store=DirectoryObjectStore(Path(tmp)/(name+'-'+mode)); start=time.perf_counter(); record=store_merkle_value(store,value); put=time.perf_counter()-start
   start=time.perf_counter(); assert restore_stored_value(store,record.digest)==value; get=time.perf_counter()-start
   row[mode]={'objects':record.objects,'bytes':record.encoded_bytes,'write_seconds':put,'restore_seconds':get}
  rows.append(row); print(json.dumps(row),flush=True)
Path('/tmp/fr-cache-storage-baseline.json').write_text(json.dumps(rows,indent=2))
