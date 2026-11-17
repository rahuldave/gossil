from pathlib import Path
import json,hashlib,re,collections,subprocess
R=Path(__file__).parent;INV=Path('/Users/rahul/Projects/AIProjectInfra/docs/gossil/evidence/fossil-inventory.json');SRC=Path('/tmp/gossil-fossil-baseline-20261117/pristine');A=json.loads(INV.read_text());D=json.loads((R/'inventory.json').read_text());P=json.loads(Path('/tmp/gossil-fossil-baseline-20261117/pristine-files.json').read_text());CAP=json.loads(Path('/tmp/gossil-fossil-baseline-20261117/capability-inventory.json').read_text());checks=[]
def check(name,condition,details=None):
 checks.append({'check':name,'passed':bool(condition),'details':details});assert condition,(name,details)
rows=D['files'];paths=[x['path'] for x in rows];by={x['path']:x for x in rows};original={x['path']:x['sha256'] for x in A['files']}
check('exact984file set with no duplicates',len(paths)==len(set(paths))==984 and set(paths)==set(original))
check('all metadata pins exact',D['source_revision']==A['source_revision'] and all(x['source_revision']==A['source_revision'] for x in rows))
check('hashes equal both original inventory and pristine manifest',all(x['sha256']==original[x['path']]==P[x['path']] for x in rows))
bad=[x['path'] for x in rows if hashlib.sha256((SRC/x['path']).read_bytes()).hexdigest()!=x['sha256']]
check('actual pristine bytes match all984 hashes',not bad,bad)
check('required surface counts',all(sum(x['path'].startswith(k+'/') for x in rows)==n for k,n in [('src',229),('test',81),('tools',78)]))
allowed={'port','replace-with-equivalent','supporting-source'};check('normative disposition enum',all(x['disposition'] in allowed for x in rows))
questions={x['id'] for x in D['required_contract_questions']}
check('every file has role destination rationale contract and resolving followup',all(x['role'] and x['proposed_destination'] and x['rationale'] and x['coverage']['required_contract'] and x['coverage']['missing_test_followup'] in questions for x in rows))
check('every file behavioral audit explicitly pending',all(x['coverage']['full_contract_audit']=='pending' for x in rows))
engine=[x for x in rows if re.match(r'^src/[^/]+\.c$',x['path'])]
check('no root engine C module silently supporting-source',all(x['disposition'] in ['port','replace-with-equivalent'] for x in engine))
expected_suites={p[5:-5] for p in A['test_suites']};actual_suites={x['path'][5:-5] for x in rows if x['path'].startswith('test/') and x['path'].endswith('.test')}
check('all37 Tcl suites and original states mapped',actual_suites==expected_suites and len(actual_suites)==37)
hooks=D['declared_diagnostic_hooks'];declared={(x['name'],x['path'],x['line']) for x in A['diagnostic_command_declarations']};mapped={(x['declaration_name'],x['source_path'],x['source_line']) for x in hooks}
check('all161 declarations mapped exactly',declared==mapped and len(hooks)==161)
check('all hooks source hash pin destination followup present',all(x['source_sha256']==original[x['source_path']] and x['source_revision']==A['source_revision'] and x['proposed_destination'] and x['missing_test_followup']['status']=='pending' for x in hooks))
check('declaration source lines exact',all(re.search(r'COMMAND:\s+'+re.escape(x['declaration_name'])+r'(?:\s|$)',(SRC/x['source_path']).read_text().splitlines()[x['source_line']-1]) for x in hooks))
runtime=D['runtime_diagnostic_hooks'];native=set(CAP['native_test_command_names'])
check('all158 runtime hook names mapped exactly',len(runtime)==158 and {x['name'] for x in runtime}==native)
check('every runtime hook maps to one pinned declaration',all(len(x['source_mapping'])==1 and x['source_mapping'][0]['canonical_name']==x['name'] for x in runtime))
check('three source-disabled/platform hook reasons present',len([x for x in hooks if x['runtime_availability']=='not-listed'])==3 and all(x['unavailable_reason'] for x in hooks if x['runtime_availability']=='not-listed'))
mandatory=['test/settings.test.off','test/update-test-1.sh','test/update-test-2.sh','test/release-checklist.wiki','src/fuzz.c','tools/sqlcompattest.c','test/tester.tcl']
check('manual dormant fuzz database-compat and disabled-driver explicitly accounted',all(by[p]['notes'] or by[p]['coverage']['missing_test_followup'] for p in mandatory))
check('baseline not relabeled allpassed',D['existing_reference_baseline']['states']=={'pass':25,'blocked':2,'fail':5,'feature-disabled':4,'not-run':1})
check('configuration matrix includes required feature/platform surfaces',{'TLS','FUSE','JSON','TH1-DOCS-HOOKS','TCL-BRIDGE','DELTA-CKSUM','FUZZ-SANITIZERS','PLATFORM-POSIX','PLATFORM-WINDOWS-CYGWIN','SQLITE-LIBSQL','MANUAL-RELEASE-DEPLOY'}<={x['id'] for x in D['configuration_matrix']})
check('no phase1 completion claim',json.loads((R/'short-report.json').read_text())['phase1_complete_claim'] is False)
result={'status':'passed exhaustive identity/disposition/hook consistency checks; not behavioral verification','checks':checks,'check_count':len(checks),'file_count':len(rows),'engine_root_C_modules':len(engine),'surface_counts':dict(collections.Counter(x['surface'] for x in rows)),'disposition_counts':dict(collections.Counter(x['disposition'] for x in rows)),'declarations':len(hooks),'runtime_hooks':len(runtime),'remaining_contract_family_questions':len(questions),'recommendation':'Use this checked inventory for independent phase1 review; keep behavioral mappings and missing configuration evidence open.'};(R/'consistency.json').write_text(json.dumps(result,indent=2));print(json.dumps({k:result[k] for k in ['status','check_count','file_count','engine_root_C_modules','declarations','runtime_hooks','remaining_contract_family_questions']},indent=2))
