import pathlib,json,subprocess,hashlib,tarfile,io,re,collections,shutil
ROOT=pathlib.Path('/Users/rahul/Projects/gest');OUT=pathlib.Path('/tmp/gossil-gest-dispositions-20261117');BASE=pathlib.Path('/tmp/gest-baseline-rGsljN');PIN='eaf821b19c3cf75b99d22e1cc70e0eff4a3562a8'
def load(p):return json.loads(pathlib.Path(p).read_text())
def dump(name,data):(OUT/name).write_text(json.dumps(data,indent=2)+'\n')
def sha(b):return hashlib.sha256(b).hexdigest()
assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()==PIN
archive=subprocess.check_output(['git','archive',PIN],cwd=ROOT)
with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
 entries=list(tar); links={t.name:t.linkname for t in entries if t.issym()}; pinned={t.name:(t.linkname.encode() if t.issym() else tar.extractfile(t).read()) for t in entries if t.isfile() or t.issym()}
tree={}
for row in subprocess.check_output(['git','ls-tree','-r','-z',PIN],cwd=ROOT).split(b'\0'):
 if row:
  meta,name=row.split(b'\t',1);mode,kind,oid=meta.decode().split();tree[name.decode()]={'mode':mode,'object_id':oid}
tracked=subprocess.check_output(['git','ls-files','-z'],cwd=ROOT).decode().rstrip('\0').split('\0');assert set(tracked)==set(pinned)
oldfiles={x['path']:x['sha256'] for x in load(BASE/'source-files.json')};assert set(oldfiles)==set(pinned)
for p,b in pinned.items():
 if p in links:
  assert sha(b)==sha(__import__('os').readlink(ROOT/p).encode()),p
  assert oldfiles[p]==sha((ROOT/p).read_bytes()),p
 else:assert sha(b)==oldfiles[p]==sha((ROOT/p).read_bytes()),p
runs=load(BASE/'individual-results.json');units=next(r for r in runs if r.get('name')=='unit-parallel')['individual_results'];integration=next(r for r in runs if r.get('name')=='integration')['individual_results'];tests=units+integration;assert len(units)==867 and len(integration)==347
byfile=collections.defaultdict(list)
for t in tests:byfile[t['source_path']].append(t['id'])
# Fresh discovery only, no assertions rerun.
for target,binary,expected in [('unit','gest-fed9f0d1a3d9ef04',units),('integration','integration-76ca87f23c992d16',integration)]:
 cmd=[str(ROOT/'target/debug/deps'/binary),'--list'];p=subprocess.run(cmd,cwd=ROOT,capture_output=True,text=True);assert p.returncode==0
 (OUT/(target+'-discovered-tests.txt')).write_text(p.stdout+p.stderr)
 discovered=re.findall(r'^(\S+): test$',p.stdout,re.M);assert set(discovered)=={t['name'] for t in expected};assert len(discovered)==len(expected)
for filename in ['cli-help.json','compatibility.json','axum-routes.json','environment.json','report.json']:shutil.copyfile(BASE/filename,OUT/('baseline-'+filename))
shutil.copyfile('/tmp/gest-storage-contract/enumeration.json',OUT/'storage-enumeration-input.json')
help=load(BASE/'cli-help.json');compat=load(BASE/'compatibility.json');web=load(BASE/'axum-routes.json');storage=load('/tmp/gest-storage-contract/enumeration.json')
# Source-resident ownership and literal argv evidence are separate relation types.
def text(path):return pinned[path].decode(errors='replace')
def body_for(t):
 src=text(t['source_path']); fn=t['name'].split('::')[-1]; matches=list(re.finditer(r'\bfn\s+'+re.escape(fn)+r'\s*\(',src))
 if len(matches)!=1:return None
 start=src.find('{',matches[0].end());mask=list(src)
 tok=r'r(?P<h>\#*)".*?"(?P=h)|b?"(?:\\.|[^"\\])*"|\x27(?:\\.|[^\x27\\])\x27|//[^\n]*|/\*.*?\*/'
 for m in re.finditer(tok,src,re.S):mask[m.start():m.end()]=' '*(m.end()-m.start())
 depth=0
 for i in range(start,len(src)):
  if mask[i]=='{':depth+=1
  if mask[i]=='}':
   depth-=1
   if depth==0:return src[start:i+1]
 return None
bodies={t['id']:body_for(t) for t in tests}
argv_evidence={}
for t in integration:
 body=bodies[t['id']] or '';arr=[]
 for m in re.finditer(r'\.args\s*\(\s*\[(.*?)\]\s*\)',body,re.S):
  prefix=[]
  for token in m[1].split(','):
   sm=re.fullmatch(r'\s*"([^"\\]*)"\s*',token)
   if not sm:break
   prefix.append(sm[1])
  if prefix:arr.append(prefix)
 argv_evidence[t['id']]=arr
followups=[]
def followup(id,owner,detail):
 followups.append({'id':id,'owner':owner,'status':'required-not-yet-implemented','action':detail});return id
ENC=followup('GEST-ENCODINGS','gossil-core','Specify Gest entity immutable revision encoding, IDs, schema versions, projection rebuild, legacy import, transaction/event/undo mappings and shared/private publication rules before durable state port. Unwritten exact encodings remain blocked.')
HTTP=followup('GEST-LIVE-HTTP','gossil-web','Run mapped requests against the embedded Axum server preserving status, bodies, CSRF, headers, SSE and mutation behavior. Source registration and handler unit ownership do not establish live route acceptance.')
STYLE=followup('GEST-TEST-STYLE-ISOLATION','gest-test-adapter','Preserve assertions; isolate every test accessing process-global styling with shared test-only guard/lock or subprocess. Prior and stress failures remain visible; serial pass is not a repair.')
contracts=[]
def cli_source(cmd):
 if not cmd:return 'src/cli.rs'
 path='src/cli/commands/'+'/'.join(x.replace('-','_') for x in cmd)+'.rs'
 if path in pinned:return path
 while '/' in path:
  path=path.rsplit('/',1)[0]+'.rs'
  if path in pinned:return path
 return 'src/cli.rs'
cliids={}
for node in help['commands']:
 cmd=node['argv'][1:-1];key=' '.join(cmd) or '<root>';cid='CLI:'+key;cliids[tuple(cmd)]=cid;src=cli_source(cmd)
 refs=[tid for tid,arrs in argv_evidence.items() if cmd and any(a[:len(cmd)]==cmd for a in arrs)]
 fid=followup('FOLLOWUP:'+cid,'gossil-gest','Verify complete '+key+' argv/flag/argument/default/validation/output/exit behavior through compatibility executable; inherited literal invocation links cover only their assertions.')
 contracts.append({'id':cid,'kind':'cli-node','command':cmd,'owner':'gossil-gest','source_files':[src],'destination':'gossil-gest CLI adapter over core services','disposition':'port','observed':'compiled --help exit0','help_sha256':sha(pathlib.Path(node['help']).read_bytes()),'options_verbatim':node['options'],'arguments_verbatim':node['arguments'],'reference_test_ids':refs,'test_relation':'exact test body contains literal argv prefix; invocation alone does not establish all command semantics','reference_test_status':'pass for linked inherited cases' if refs else 'missing-direct-literal-invocation-evidence','gossil_status':'not-yet-implemented','followups':[fid,ENC]})
 if any('--json' in line for line in node['options']):
  jrefs=[tid for tid in refs if '--json' in (bodies[tid] or '') or '"-j"' in (bodies[tid] or '')]
  jf=followup('FOLLOWUP:JSON:'+key,'gossil-gest','Map every '+key+' JSON shape, null/omission/default/precedence and error output assertion; preserve envelope and command-specific deviations. No complete JSON schema inferred from output-site inventory.')
  contracts.append({'id':'JSON:'+key,'kind':'json-command','command':cmd,'owner':'gossil-gest','source_files':[src,'src/ui/json.rs','src/ui/envelope.rs'],'disposition':'port','destination':'gossil-gest JSON compatibility serializer','observed':'compiled help exposes --json','reference_test_ids':jrefs,'test_relation':'literal command invocation plus JSON flag mention in same exact test body, not exhaustive schema proof','reference_test_status':'pass for linked inherited cases' if jrefs else 'missing-direct-JSON-test-evidence','gossil_status':'not-yet-implemented','followups':[jf,ENC]})
for n,a in enumerate(compat['aliases'],1):
 alias=re.search(r'alias\s*=\s*"([^"]+)"',a['declaration'])[1]
 prefix=[] if a['file']=='src/cli.rs' else a['file'].removeprefix('src/cli/commands/').removesuffix('.rs').split('/');prefix=[x.replace('_','-') for x in prefix]
 if a['declaration'].startswith('#[arg'):
  explicit=re.search(r'long\s*=\s*"([^"]+)"',a['declaration']); canonical='--'+(explicit[1] if explicit else re.match(r'(\w+)',a['next'])[1].replace('_','-'));cmd=prefix;alias_argv=prefix+['--'+alias];canonical_argv=prefix+[canonical];subtype='option-alias'
  refs=[tid for tid,arrs in argv_evidence.items() if any(ar[:len(prefix)]==prefix for ar in arrs) and ('"--'+alias+'"') in (bodies[tid] or '')]
 else:
  variant=re.match(r'(\w+)',a['next'])[1];canonical=re.sub(r'(?<!^)(?=[A-Z])','-',variant).lower();cmd=prefix+[canonical];alias_argv=prefix+[alias];canonical_argv=cmd;subtype='command-alias'
  refs=[tid for tid,arrs in argv_evidence.items() if any(ar[:len(prefix)+1]==prefix+[alias] for ar in arrs)]
 assert tuple(cmd) in cliids,(a,cmd)
 fid=followup('FOLLOWUP:ALIAS:'+str(n),'gossil-gest','Exercise alias '+ ' '.join(alias_argv)+' and compare canonical parse/flags/output/exit; declaration existence does not prove alias invocation parity.')
 contracts.append({'id':'ALIAS:'+str(n),'kind':'cli-alias','alias_kind':subtype,'alias':alias_argv,'canonical':canonical_argv,'canonical_contract':cliids[tuple(cmd)],'owner':'gossil-gest','source_files':[a['file']],'source_line':a['line'],'declaration':a['declaration'],'disposition':'port','destination':'gossil-gest clap aliases','reference_test_ids':refs,'test_relation':'literal alias argv/option mention in exact test body','reference_test_status':'pass for linked inherited cases' if refs else 'missing-direct-alias-invocation-evidence','gossil_status':'not-yet-implemented','followups':[fid]})
for route in web['routes']:
 owners=[]
 for handler in route['methods'].values():
  if handler=='assets::serve':f='src/web/assets.rs'
  elif handler=='sse::events':f='src/web/sse.rs'
  else:
   name=handler.split('::')[-1];candidates=[p for p in pinned if p.startswith('src/web/handlers/') and re.search(r'\bfn\s+'+re.escape(name)+r'\s*\(',text(p))]
   assert len(candidates)==1,(name,candidates);f=candidates[0]
  owners.append(f)
 refs=sorted({tid for f in owners for tid in byfile[f]})
 contracts.append({'id':'HTTP:'+route['path'],'kind':'http-route','path':route['path'],'methods':route['methods'],'owner':'gossil-web','source_files':['src/web.rs']+sorted(set(owners)),'disposition':'port','destination':'gossil-web Axum route/handler adapters','observed':'source registration only','reference_test_ids':refs,'test_relation':'tests resident in handler implementation source files; not proof each route/method executes','reference_test_status':'handler-file inherited tests pass where linked; live route acceptance not established','gossil_status':'not-yet-implemented','followups':[HTTP,ENC]})
for code,desc in compat['exit_map'].items():
 refs=[t['id'] for t in integration if re.search(r'::it_exits_'+code+r'_on_',t['name'])]
 fid=followup('FOLLOWUP:EXIT:'+code,'gossil-gest','Preserve exit '+code+' for all source error variant branches and equivalent adapter errors; representative family test is not proof every branch maps correctly.')
 contracts.append({'id':'EXIT:'+code,'kind':'exit-code','code':int(code),'meaning':desc,'owner':'gossil-gest','source_files':['src/main.rs','src/cli.rs'],'disposition':'port','destination':'gossil-gest error mapping and CLI exit boundary','reference_test_ids':refs,'test_relation':'named representative exit-family integration assertion' if refs else 'supplemental invalid-command subprocess observation, not inherited assertion','reference_test_status':'pass' if refs or code=='2' else 'missing','gossil_status':'not-yet-implemented','followups':[fid]})
for m in storage['schema_migrations']:
 src=m['source'];refs=byfile['src/store/migration.rs'];tables=re.findall(r'(?i)(?:CREATE\s+TABLE(?:\s+IF\s+NOT\s+EXISTS)?|ALTER\s+TABLE)\s+([a-z_]+)',m['sql'])
 contracts.append({'id':'MIGRATION:'+str(m['version']),'kind':'storage-migration','version':m['version'],'tables':tables,'owner':'gossil-core','source_files':[src,'src/store/migration.rs'],'sql_sha256':sha(m['sql'].encode()),'disposition':'replace-with-equivalent','destination':'gossil-core Gest projection schema and legacy import adapter','rationale':'Preserve inherited schema/import behavior as compatibility input; Fossil-backed immutable records become authority, not these mutable legacy tables. Exact replacement encoding remains required-blocked.','reference_test_ids':refs,'test_relation':'tests resident in migration runner source; per-migration behavior not independently audited','reference_test_status':'pass for linked inherited cases; schema parity coverage still requires review','gossil_status':'required-blocked','followups':[ENC]})
table_sources=collections.defaultdict(list)
for c in contracts:
 if c['kind']=='storage-migration':
  for table in c['tables']:table_sources[table]+=c['source_files']
table_sources['_migrations']=['src/store/migration.rs']
for table,sources in sorted(table_sources.items()):
 contracts.append({'id':'TABLE:'+table,'kind':'storage-table','table':table,'owner':'gossil-core','source_files':sorted(set(sources)),'disposition':'replace-with-equivalent','destination':'gossil-core Gest projection/import schema','reference_test_ids':byfile['src/store/migration.rs'],'test_relation':'migration-runner source ownership, not complete table semantics proof','reference_test_status':'runner tests pass; table semantics audit required','gossil_status':'required-blocked','followups':[ENC]})
for family in storage['mutation_families']:
 src=family['source'];refs=byfile[src]+[tid for p in family['integration_test_files'] for tid in byfile[p]]
 contracts.append({'id':'MUTATION:'+family['family'],'kind':'storage-mutation-family','owner':'gossil-core','source_files':[src],'functions':family['functions'],'disposition':'replace-with-equivalent','destination':'gossil-core domain transaction service and projection adapter','reference_test_ids':sorted(set(refs)),'test_relation':'source-resident tests and supplied source-owner integration file links; assertion-to-function review pending','reference_test_status':'pass for inherited linked cases; no Fossil-backed acceptance','gossil_status':'required-blocked','followups':[ENC]})
# Enumerate infrastructure declarations explicitly rather than silently treating them as schema.
for i,item in enumerate(storage['infrastructure_mutations'],1):
 expr=item.split(':',1)[0].split(' ',1)[0]
 if '{' in expr:
  pre,rest=expr.split('{',1);names,post=rest.split('}',1);sources=[pre+n+post for n in names.split(',')]
 else:sources=[expr]
 assert set(sources)<=set(pinned)
 refs=sorted({tid for source in sources for tid in byfile[source]})
 contracts.append({'id':'INFRA-MUTATION:'+str(i),'kind':'storage-infrastructure-mutation','owner':'gossil-core','source_declaration':item,'source_files':sources,'disposition':'replace-with-equivalent','destination':'gossil-core transactions/projections or gossil-gest legacy import','reference_test_ids':refs,'reference_test_status':'source-resident inherited tests pass where linked; function-level semantic mapping pending','test_relation':'source-resident tests, not validated function acceptance','gossil_status':'required-blocked','followups':[ENC]})
file_contracts=collections.defaultdict(list)
for c in contracts:
 for p in c['source_files']:file_contracts[p].append(c['id'])
for f in ['src/ui/json.rs','src/ui/envelope.rs']:
 cid='JSON-SHARED:'+f;fid=followup('FOLLOWUP:'+cid,'gossil-gest','Preserve all serialized field/flatten/omit rules and output-mode precedence in '+f+'; resolve every command caller to shared or custom serialization with exact assertion mapping.')
 c={'id':cid,'kind':'json-shared-output','owner':'gossil-gest','source_files':[f],'disposition':'port','destination':'gossil-gest compatible JSON serializers','source_sha256':sha(pinned[f]),'reference_test_ids':byfile[f],'test_relation':'source-resident inherited tests, not exhaustive caller shape acceptance','reference_test_status':'pass for linked inherited cases' if byfile[f] else 'missing-direct-tests','gossil_status':'not-yet-implemented','followups':[fid]};contracts.append(c);file_contracts[f].append(cid)
for file in compat['json_output_source_sites']:
 for hit in file['json_output_sites']:
  f=file['file'];cid='JSON-OUTPUT-SITE:'+f+':'+str(hit['line']);refs=byfile[f]
  contracts.append({'id':cid,'kind':'json-output-site','owner':'gossil-gest','source_files':[f],'source_line':hit['line'],'source_expression':hit['text'],'disposition':'port','destination':'gossil-gest JSON serializer or core-value adapter','reference_test_ids':refs,'test_relation':'source-resident test relation only; source expressions may prepare values rather than emit JSON','reference_test_status':'pass for linked inherited cases' if refs else 'missing-exact-output-assertion-mapping','gossil_status':'not-yet-implemented','followups':[ENC]});file_contracts[f].append(cid)

def disposition(p):
 if p.startswith('src/store/migration/') or p=='src/store/migration.rs':return 'replace-with-equivalent','gossil-core','gossil-core/src/gest_projection/schema/'+p.removeprefix('src/store/'),'Legacy mutable schema is compatibility input; preserve imports/IDs/constraints through immutable record projections; exact encodings remain blocked.',True
 if p.startswith('src/store/repo/') or p=='src/store/repo.rs':return 'replace-with-equivalent','gossil-core','gossil-core/src/gest_projection/'+p.removeprefix('src/store/'),'Retain domain query/mutation/error semantics while core owns immutable revisions, transactions and rebuildable SQL projections.',True
 if p.startswith('src/store/sync/') or p=='src/store/sync.rs':return 'replace-with-equivalent','gossil-gest','gossil-gest/src/import/legacy_'+p.removeprefix('src/store/'),'Preserve legacy sidecar import compatibility; Fossil core supplies versioned storage and sync. Sidecar identities and conflict semantics need an explicit mapped replacement, not omission.',True
 if p.startswith('src/store/model/') or p=='src/store/model.rs':return 'port','gossil-gest','gossil-gest/'+p,'Preserve Gest public model fields, IDs/status/enums/serialization; map persistence to core revision/projection interfaces.',True
 if p.startswith('src/store') or p.startswith('src/actions'):return 'replace-with-equivalent','gossil-core','gossil-core/src/gest_services/'+p.removeprefix('src/'),'Move durable operations under Fossil-backed core transactions; preserve Gest domain outcomes and error behavior.',True
 if p.startswith('src/web') or p.startswith('templates/'):
  return 'port','gossil-web','gossil-web/'+p,'Preserve embedded Axum/Tokio server, HTTP transport, templates/assets/styles/Markdown renderer; handlers call core services instead of mutable Gest authority.',p.startswith('src/web/handlers') or p in ['src/web.rs','src/web/state.rs']
 if p.startswith('src/cli'):
  return 'port','gossil-gest','gossil-gest/'+p,'Preserve public commands, flags, aliases, output/JSON/exit behavior and IDs; adapter dispatches to core services.',True
 if p=='src/main.rs':return 'port','gossil-cli','gossil-cli/src/main.rs','Preserve startup/parse/error dispatch semantics through shared core; explicit gest compatibility executable must not replace installed Gest automatically.',True
 if p.startswith('src/ui'):return 'port','gossil-gest','gossil-gest/'+p,'Preserve terminal components, styling, output modes and Markdown behavior; test global-style isolation must preserve assertions.',False
 if p.startswith('src/config') or p.startswith('src/io') or p=='src/logging.rs':return 'port','gossil-gest','gossil-gest/'+p,'Preserve configuration precedence, IO/editor/pager and logging behavior; adapt only package paths and explicit storage-owner boundaries.',False
 if p.startswith('tests/'):
  return 'port','gest-test-adapter','tests/gest-reference/'+p.removeprefix('tests/'),'Retain every inherited assertion/fixture and stable test identity; adapt imports/setup/invocation only, document required substitutions and styling isolation.',False
 if p.startswith('.agents/') or p=='AGENTS.md':return 'supporting-source','gossil-development','reference/gest/'+p,'Retain workflow provenance as development reference; do not install or overwrite installed skills or treat instructions as runtime dependencies.',False
 if p.startswith('docs/') or p in ['README.md','CHANGELOG.md']:return 'supporting-source','gossil-docs','docs/reference/gest/'+p,'Retain upstream specification/design/user/developer evidence and attribution; reconcile public compatibility documentation with explicit replacement contracts.',False
 if p=='LICENSE':return 'supporting-source','gossil-licensing','LICENSES/Gest-MIT.txt','Retain upstream license and required notices with ported source.',False
 if p in ['Cargo.toml','Cargo.lock','build.rs']:return 'supporting-source','gossil-build','reference/gest-build/'+p,'Retain dependency/build provenance; chosen Gossil crates and libsql/native versions require new locked-build evidence.',False
 return 'supporting-source','gossil-development','reference/gest/'+p,'Retain tracked repository/build/release/style/policy support in source accounting; assign equivalent project setup deliberately rather than assuming active installation.',False
files=[]
for p in sorted(pinned):
 d,owner,dest,why,blocked=disposition(p);refs=byfile[p];cs=file_contracts[p];comment=next((l.strip().removeprefix('//!').strip() for l in text(p).splitlines()[:12] if l.startswith('//!')),'')
 if p.startswith('tests/'):role='inherited integration assertion source' if refs else 'integration fixture/helper/module support'
 elif p.endswith('.rs'):role=comment or 'Rust '+pathlib.PurePosixPath(p).stem+' module or build source'
 elif p.startswith('templates/'):role='Gest dashboard template or browser asset'
 elif p.startswith('docs/') or p.endswith('.md'):role='documentation or agent instruction source'
 else:role='tracked build/configuration/license/support artifact'
 fus=[]
 if blocked:fus.append(ENC)
 if p.startswith('src/ui') and ('yansi::disable()' in text(p) or 'yansi::enable()' in text(p)):fus.append(STYLE)
 # A direct-test absence is explicit even when contract links exist: links are not substitutes for assertions.
 if not refs:
  fid='FILE-FOLLOWUP:'+p;action=('Review source-owner contracts and map exact behavior tests or write missing mapped compatibility assertions for '+p+'.' if d!='supporting-source' else 'Verify retained provenance/license/build/documentation role for '+p+'; select an appropriate non-runtime validation, no runtime test coverage claimed.')
  fus.append(followup(fid,owner,action))
 files.append({'path':p,'sha256':sha(pinned[p]),'git_mode':tree[p]['mode'],'git_object_id':tree[p]['object_id'],'git_blob_kind':'symlink' if p in links else 'regular-file','link_target':links.get(p),'resolved_content_sha256':sha((ROOT/p).read_bytes()) if p in links else None,'prior_inventory_hash_basis':'resolved-target-bytes' if p in links else 'regular-file-bytes','size':len(pinned[p]),'role':role,'disposition':d,'owner':owner,'gossil_destination':dest,'destination_status':'proposed-required-blocked-on-encoding' if blocked else 'proposed-not-implemented','rationale':why,'reference_test_ids':refs,'test_relation':'exact compiled tests resident in this source file' if refs else 'no source-resident compiled test; missing-test/validation follow-up recorded','reference_test_status':'pass in prior recorded reference run; styling race caveat applies to UI tests' if refs else 'not-established','contract_ids':cs,'followup_ids':fus})
SEMANTIC=followup('GEST-ASSERTION-SEMANTIC-MAPPING','gest-test-adapter','For every referenced compiled test identity, review its exact assertions and map required domain behavior; source ownership and literal argv invocation links are traceability, not exhaustive behavioral coverage.')
bytestcontracts=collections.defaultdict(list)
for c in contracts:
 for tid in c['reference_test_ids']:bytestcontracts[tid].append(c['id'])
# All compiled tests have an explicit transplant destination, parent file link, status and retained assertion commitment.
test_records=[]
for t in tests:
 assert t['source_path'] in pinned,t
 record={**t,'source_sha256':sha(pinned[t['source_path']]),'owner':'gest-test-adapter','disposition':'port','gossil_destination':'tests/gest-reference/'+t['source_path'],'assertion_policy':'retain unchanged; setup/import/target adaptations recorded separately','reference_status':'pass','gossil_status':'not-yet-implemented','contract_ids':sorted(set(bytestcontracts[t['id']]+file_contracts[t['source_path']])),'coverage_relation':'test identity and containing source; semantic assertion-to-contract audit not complete','followup_ids':[SEMANTIC]+([STYLE] if t['source_path'].startswith('src/ui/') else []),'coverage_followup_action':'Review exact assertions for '+t['id']+' and complete semantic mapping before claiming required behavioral acceptance'}
 test_records.append(record)
# Supplemental race observations do not overwrite inherited default pass status.
race=load(BASE/'report.json')['race_characterization']
manifest={'schema_version':1,'inventory_kind':'exhaustive-file/test/API-source-dispositions-not-semantic-audit','pin':PIN,'ownership':'Only /tmp/gossil-gest-dispositions-20261117 written; reference and Gossil sources/task state untouched.','boundaries':['Fixed substitutions preserve Gest API, IDs, exit behavior, terminal/browser styles, Markdown renderer and embedded Axum/Tokio server.','Durable state becomes Fossil-backed core authority; mutable Gest tables and sidecar sync require equivalent mapped projections/import semantics, not deletion.','Destinations are proposed architecture locations, not implemented files.','Exact entity encodings, projection/import/rebuild and shared/private publication contracts remain required-blocked.','File and contract-set completeness does not establish implementation, exhaustive semantic behavior audit, or live HTTP acceptance.','This component does not complete phase1 or authorize phase2 engine work by itself.'],'inputs':{'baseline':'/tmp/gest-baseline-rGsljN/report.json','storage':'/tmp/gest-storage-contract/enumeration.json','pinned_archive_sha256':sha(archive)},'counts':{'files':len(files),'unit_tests':len(units),'integration_tests':len(integration),'contracts':dict(collections.Counter(c['kind'] for c in contracts))},'files':files,'tests':test_records,'contracts':contracts,'followups':followups,'styling_race':race,'recommendations':['Retain all source and inherited assertions under these dispositions; complete semantic contract review instead of treating mapping as acceptance.','Resolve exact Fossil-backed Gest record/projection/import encodings before replacing durable storage.','Port public CLI/JSON/exit and embedded web/style/Markdown behavior using mapped tests and explicit missing coverage; keep live route acceptance pending.']}
dump('inventory.json',manifest)
# Independent set/count/foreign-key validation, including pin blob checks and fresh compiled test discovery above.
filekeys={f['path'] for f in files};testkeys={t['id'] for t in test_records};contractkeys={c['id'] for c in contracts};followupkeys={f['id'] for f in followups}
assert len(filekeys)==len(files)==544 and filekeys==set(pinned)==set(tracked)
assert len(testkeys)==len(tests)==1214 and {t['id'] for t in tests}==testkeys
assert len(contractkeys)==len(contracts)
assert {tuple(c['command']) for c in contracts if c['kind']=='cli-node'}=={tuple(n['argv'][1:-1]) for n in help['commands']}
assert {tuple(c['command']) for c in contracts if c['kind']=='json-command'}=={tuple(n['command']) for n in compat['commands_with_json']}
assert {c['path'] for c in contracts if c['kind']=='http-route'}=={r['path'] for r in web['routes']}
assert len([c for c in contracts if c['kind']=='cli-alias'])==len(compat['aliases'])==29
assert {c['version'] for c in contracts if c['kind']=='storage-migration'}==set(range(1,16))
assert {m['source'] for m in storage['schema_migrations']}=={p for p in pinned if re.fullmatch(r'src/store/migration/m\d+_.*\.rs',p)}
assert {(c['source_files'][0],c['source_line'],c['declaration']) for c in contracts if c['kind']=='cli-alias'}=={(a['file'],a['line'],a['declaration']) for a in compat['aliases']}
assert {c['table'] for c in contracts if c['kind']=='storage-table'}==set(table_sources)
for t in test_records:
 assert set(t['contract_ids'])<=contractkeys and set(t['followup_ids'])<=followupkeys
assert {c['code'] for c in contracts if c['kind']=='exit-code'}==set(map(int,compat['exit_map']))
for f in files:
 assert f['disposition'] in {'port','replace-with-equivalent','supporting-source'}
 assert f['reference_test_ids'] or f['followup_ids'],f['path']
 assert set(f['reference_test_ids'])<=testkeys and set(f['contract_ids'])<=contractkeys and set(f['followup_ids'])<=followupkeys
for c in contracts:
 assert set(c['reference_test_ids'])<=testkeys and set(c['source_files'])<=filekeys and set(c['followups'])<=followupkeys
assert subprocess.check_output(['git','status','--short'],cwd=ROOT,text=True)=='?? .gest/\n'
validation={'status':'pass','checks':{'pinned_revision':PIN,'tracked_set_exact':len(filekeys),'tracked_hashes_match_pin_and_prior_inventory':{'regular_files':541,'symlinks':3,'note':'Pinned symlink blob hashes represent link-target text; prior inventory hashed resolved document bytes, recorded separately.'},'fresh_compiled_discovery_matches_all_tests':{'unit':867,'integration':347},'cli_node_set_exact':93,'json_node_set_exact':72,'alias_count_exact':{'declarations':29,'command_aliases':27,'option_aliases':2},'route_path_set_exact':34,'migration_versions_exact':list(range(1,16)),'derived_tables':sorted(table_sources),'exit_code_set_exact':sorted(map(int,compat['exit_map'])),'all_foreign_keys_resolve':True,'every_file_has_tests_or_explicit_followup':True,'disposition_values_normative':True,'source_status_unchanged':True},'inventory_sha256':sha((OUT/'inventory.json').read_bytes()),'limits':'Set completeness and structural referential integrity only; no new semantic or live HTTP acceptance, no engine implementation and no full phase1 completion.'}
dump('validation.json',validation)
report='''Gest disposition inventory component\n\nObserved: pinned Gest revision and all 544 tracked entries match the pin; 541 regular-file hashes match prior reference evidence and three symlinks now distinguish pinned link-target blobs from resolved-content hashes. Fresh compiled discovery matches 867 unit and 347 integration identities; no assertions were rerun. Every file has a role, owner, proposed destination, normative disposition, rationale and exact source-resident test relation or explicit follow-up.\n\nMapped API surfaces: 93 canonical CLI nodes including root, 72 JSON nodes, 29 alias declarations (27 command and two option aliases), 34 Axum paths with methods, exit families, shared JSON helpers/output sites, 15 schema migrations and derived tables. Contract links distinguish literal invocation or source ownership from behavioral acceptance; every unproven contract retains a follow-up.\n\nProposed: keep CLI/API/IDs/exits/styles/Markdown and embedded server behavior while routing durable state through Fossil core records and rebuildable projections. Exact encodings/import/rebuild/private-shared rules remain required-blocked. No file is omitted or dropped. Source registration does not establish live HTTP acceptance.\n\nPrior executed evidence: default parallel and serial full unit runs passed 867 each, integration passed 347. Bounded concurrent UI tests reproduced the original task-row assertion. Global styling test interference is a source-supported inference; rendered ANSI bytes were not captured. Assertions remain unchanged.\n\nValidation: validation.json passes complete pinned-file, compiled-test and API-set checks plus all inventory foreign keys. The inventory provides a complete mapping. Semantic behavior review, implementation and phase1 acceptance remain pending.\n\nRecommendations\n\nPrefer unchanged inherited assertions with explicit compatibility adapters and global-style test isolation. Next, resolve blocked Gest/Fossil storage encodings and fill exact semantic/live-HTTP coverage before claiming acceptance.\n'''
(OUT/'report.txt').write_text(report)
print(json.dumps(validation,indent=2))
