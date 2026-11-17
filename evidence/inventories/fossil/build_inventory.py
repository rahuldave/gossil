from pathlib import Path
import json,hashlib,re,collections
R=Path(__file__).parent; ROOT=Path('/tmp/gossil-fossil-baseline-20261117/pristine');SPEC=Path('/Users/rahul/Projects/AIProjectInfra/docs/gossil/evidence/fossil-inventory.json');I=json.loads(SPEC.read_text());PIN=I['source_revision'];B=json.loads(Path('/tmp/gossil-fossil-baseline-20261117/baseline-240s.json').read_text());CAP=json.loads(Path('/tmp/gossil-fossil-baseline-20261117/capability-inventory.json').read_text());BASE={x['suite']:x for x in B['suite_results']}
# These are proposed logical responsibility boundaries, not claims about an existing Rust workspace.
F={}
def family(key,stems,contract,destination,tests=(),disposition='port',followup=None):
 F[key]={'id':key,'required_contract':contract,'proposed_destination':destination,'disposition':disposition,'candidate_inherited_suites':list(tests),'missing_test_followup':followup or 'Derive focused positive, malformed-input and state/integrity tests for '+contract+'; inherited suite references alone do not establish complete coverage.'}
 for stem in stems.split():
  assert stem not in MODULES,(stem,key);MODULES[stem]=key
MODULES={}
family('checkout','add checkin checkout vfile file winfile utf8 unicode undo','versioned filesystem changes, path/permission/encoding handling, checkout state and undo','Rust checkout/filesystem core', ['clean','file1','mv-rm','reserved-names','revert','symlinks','update','utf','commit-warning'])
family('history','branch descendants leaf name finfo pivot','artifact name resolution, branches, leaves, ancestry and file history','Rust history/name graph core',['amend','update','merge1'])
family('merge','merge merge3 stash patch bisect','merge/conflict/rename behavior, stash, patch exchange and bisect state','Rust merge/worktree core',['merge1','merge2','merge3','merge4','merge5','merge6','merge_exe','merge_renames','merge_renames_2','merge_warn','stash','update'])
family('diff','diff diffcmd','diff algorithms, binary detection, output and diagnostic behavior','Rust diff core and CLI/Gest presentation adapters',['diff','stash'])
family('storage','content manifest rebuild verify shun purge','artifact parsing, crosslinks, reconstruction, integrity, shunning and purge semantics','Rust repository artifact/integrity core',['set-manifest','amend','update'])
family('database','db schema sqlcmd foci deltafunc','repository/local/config schemas, transactions, migrations, SQL interfaces and Fossil virtual/scalar functions','Rust repository core using selected libsql family',['settings-repo','json','set-manifest'],disposition='adapt',followup='Inventory SQL functions, virtual tables, compile options, transactions/locking and schema upgrades; qualify libsql against tools/sqlcompattest.c requirements and crash/rollback fixtures without a parallel SQLite dependency.')
family('bytes','blob delta deltacmd encode gzip tar zip bundle','binary blobs, delta codec, compression envelopes, archive/bundle formats and safe extraction','Rust byte/codec/archive primitives',['delta1'],followup='Complete malformed delta/encoder interoperability, compression envelope failures, archive paths/bounds and bundle roundtrip tests; delta decoder corpus only characterizes its finite cases.')
family('hashes','hname md5 sha1 sha1hard sha3','artifact hash policy, collision-detecting SHA1, SHA3 variants and legacy MD5 use','Rust content hash/name primitives',[],followup='Record hash known-answer and collision-detection vectors, policy promotion/reuse and nonstandard SHA3 length behavior; SHA256 fixture hashes are not artifact hash coverage.')
family('sync','clone sync xfer http http_socket http_ssl http_transport url','clone/pull/push/sync protocols, transports, TLS/SSH/proxy/authentication and recovery','Rust replication/transport core',['unversioned'],followup='Add C/Rust bidirectional xfer fixtures for every artifact family, private/shunned content, divergence/interruption/retry, TLS certificates and SSH/proxy failures; unversioned suite is not general sync coverage.')
family('records','attach event forum tkt tag unversioned wiki backlink','attachments, technotes, forum/ticket/wiki/tag/unversioned artifacts and backlinks','Rust repository record mechanisms',['wiki','unversioned','amend'],followup='Map each artifact/card parser, crosslink, edit/moderation/reply/deletion/history/sync behavior to fixtures; wiki/unversioned assertions do not cover all record families.')
family('auth','capabilities captcha cookies login setupuser user security_audit moderate','users, capabilities, authentication, sessions, moderation and administrative security','Rust auth/moderation core with Gest server adapters',['th1','json'],followup='Add capability matrix, session/password/login-group/CSRF/captcha/moderation and adversarial privilege tests; inherited TH1/JSON references are partial and JSON build is disabled.')
family('service','alerts backoffice smtp hook','mail alerts/delivery, backoffice jobs and external event hooks','Rust alerts/jobs/hook core',[],followup='Preserve queues/retries/mail encoding/auth/subscriber permissions/hook ordering and subprocess behavior; adapt fake SMTP/email tools into owned-fixture tests before execution.')
family('server','ajax cgi dispatch etag extcgi loadctrl main repolist robot xfersetup','HTTP routing/CGI/extensions, cache validators, load/bot control, process lifecycle and replication setup','Gest-derived embedded Rust server with required Fossil protocol/admin adapters',['cmdline','wiki','th1','unversioned'],disposition='adapt',followup='Map every CLI/web route and option to embedded-server/API equivalents, including CGI/SCGI/extension deployment compatibility decisions, resource limits and child lifecycle; server substitution cannot delete xfer/security/extension mechanisms.')
family('render','markdown markdown_html','Markdown rendering and engine-specific output/security semantics','Gest Markdown engine; explicit Fossil-to-Gest rendering contract mapping',['th1'],disposition='adapt',followup='Define intentional rendering differences and replacement assertions against chosen Gest Markdown engine; retain source examples, security/link/HTML behavior and observed th1-markdown-5 failure.')
family('wiki-render','wikiformat interwiki lookslike comformat','Fossil wiki parsing/rendering, interwiki links, content detection and comment formatting','Rust wiki/content-format core with Gest display adapter',['wiki','th1'],followup='Gest Markdown substitution does not replace Fossil wiki parser or artifact semantics. Map formatting/link/safety rules and retain HTML/wiki/Markdown fixtures.')
family('scripting','th th_lang th_main th_tcl','TH1 language, command bindings, docs/hooks/taint and Tcl bridge','Rust scripting compatibility layer; interpreter/bridge implementation decision pending',['th1','th1-docs','th1-hooks','th1-repo','th1-taint','th1-tcl'],followup='Specify interpreter/FFI lifecycle, Tcl8.6/9/stubs, taint/escaping/permissions, docs/hooks and error parity. Disabled reference features remain required configurations, not exclusions.')
family('query','regexp glob match search','regular expressions, glob/match modes and search/index behavior','Rust query/search/matching core',['glob','th1'],followup='Preserve Unicode/case/glob/regexp/SQL matching and search-index consistency; add differential malformed and boundary tests, full-text query/result/permission tests.')
family('analytics','graph timeline stat statrep report piechart rss sitemap path pqueue bag cache','graph traversal/layout, timeline/report/stat/search-support algorithms, collections/cache and feeds','Rust graph/query/collections core plus Gest visual adapters',[],followup='Separate underlying graph/report/feed/caching algorithms from substituted presentation; map ordering/filter/permission/invalidation/SQL-report and graph fixtures to contract tests.')
family('ui-workflows','browse doc fileedit info setup tktsetup style skins color menu','repository browsing, embedded docs, editing/settings/ticket customization and presentation orchestration','Gest UI/server views preserving required repository/admin workflows',[],disposition='adapt',followup='Map each upstream page/action to chosen Gest UI/API flow and required engine operation, including edit conflicts, permissions, docs paths and customization; reduced UI is not an engine exemption.')
family('process','allrepo clearsign configure export import publish popen xsystem fshell terminal util printf','multi-repository operations, signing, config exchange, import/export/publish, shell/process/terminal and utility contracts','Rust CLI/OS/process adapters and repository interchange core',[],followup='Specify signing/verification, Git/SVN/import/export/mirror data parity, external command quoting/errors, multi-repository isolation and terminal formatting; add owned-process fixtures across platforms.')
family('platform-fuse','fusefs','read-only virtual filesystem/FUSE repository browsing','Rust platform/FUSE adapter; selected reference build disabled',[],followup='Qualify FUSE-enabled reference and Rust behavior, mount/unmount/path/permission/history reads, platform availability and interruption; disabled build is not removed scope.')
family('platform-windows','winhttp','Windows HTTP/service/process transport behavior','Rust Windows platform/server adapter; support boundary decision remains explicit',[],disposition='defer-required',followup='Resolve supported Windows contract and equivalent platform behavior before full port acceptance; run Windows native service/HTTP fixtures if required. Keep source accounted even though current host is macOS.')
family('test-infrastructure','fuzz','libFuzzer entrypoints for parser/render/codec mechanisms','Rust fuzz targets mapped to the same mechanism',[],disposition='adapt',followup='Enumerate FUZZ_* dispatch choices from source, retain seed corpus, qualify Linux sanitizer/libFuzzer and Rust fuzz equivalents; no fuzz campaign executed in this inventory.')
# C helpers with roles outside runtime engine.
family('build-generation','builtin','embedded resource registry/loading','Rust resource embedding pipeline and runtime lookup contract',[],disposition='adapt',followup='Compare embedded resource names/bytes/cache behavior and generated-resource reproducibility; retain source assets and licenses.')
# Any explicitly omitted C stem is a generation error, not an automatic defer.
# Contract questions are explicit; each file/hook links to one.
QUESTIONS={k:{'id':'CONTRACT-'+k,'family':k,'question':v['missing_test_followup'],'status':'open required acceptance mapping'} for k,v in F.items()}
family('json-api','json json_artifact json_branch json_config json_diff json_dir json_finfo json_login json_query json_report json_status json_tag json_timeline json_user json_wiki','Fossil JSON envelopes/endpoints, values, errors, permissions and repository actions','Gest-derived Rust server API compatibility adapter',['json'],disposition='adapt',followup='Build reference with JSON enabled; enumerate every JSON subcommand and envelope/error/auth contract, map to preserved public endpoint or explicit requirement-based replacement assertion. Current JSON suite skipped.')
family('update','update','checkout update/merge state transitions, local deletes and conflicts','Rust checkout update core',['update','merge1','merge_renames'],followup='Preserve local changes and all conflict/update outputs; retain observed update-message-add-overwrites failure, add crash/undo and cross-checkout state fixtures.')
family('chat','chat','ephemeral chat storage, poll/send/delete/download and permissions','Rust chat mechanisms with Gest server/UI adapter',[],followup='Explicitly map storage retention, cursor/poll delivery, attachment/download and permission/privacy semantics; transient chat is still required engine scope.')
family('diagram','pikchrshow','Pikchr diagram processing and rendering endpoint behavior','Rust diagram/render service with Gest presentation adapter',[],followup='Choose compatible Pikchr execution strategy and test rendering/errors/resource limits/embedded integration. Gest Markdown replacement alone does not replace diagrams.')
for key,contract,destination,follow in [
 ('build','reference acquisition, source generation and reproducible build/deployment settings','reference build/provenance tooling plus Rust Cargo/Just contract','Map build options/generated resources/platform targets to reproducible Rust/reference configurations; do not treat build documentation as implemented gates.'),
 ('docs','upstream documentation, examples and release/operation guidance','pinned upstream reference documentation and Gossil contract crosswalk','Extract behavioral claims/examples from each retained page and map to tests; file inventory is not a page-by-page behavioral audit.'),
 ('assets','UI artwork, skins, styles, sounds and visual interaction assets','pinned reference asset catalog plus Gest UI equivalents','Decide equivalent Gest interaction/visual/accessibility/asset/license mapping; retain related engine workflow contracts and add visual/workflow tests.'),
 ('dependency','bundled Tcl/zlib/SQLite/cson/line editing/diagram compatibility sources','pinned dependency provenance/reference build; Rust/libsql/engine adapters as specified','Resolve licenses, features/ABI/compile flags and differential dependency behavior; libsql replaces SQLite family, while Tcl/diagram/compression mechanisms remain required.'),
 ('migration-tools','CVS history/revision/symbol conversion tooling','pinned migration-tool reference; required import/interchange contract decision','Determine standalone CVS-tool support and preserved import result semantics; create fixture repositories for every conversion stage/cycle/symbol edge before any rewrite.'),
 ('test-surfaces','test harness, fixtures, manual scripts, tools and dormant tests','pristine inherited test/support tree and separately patched runner','Retain exact fixtures and classify runnable/manual/dormant/build/platform prerequisites; convert manual observations into assertions under owned resources before reporting passes.')]:
 F[key]={'id':key,'required_contract':contract,'proposed_destination':destination,'disposition':'reference','candidate_inherited_suites':[],'missing_test_followup':follow}
QUESTIONS={k:{'id':'CONTRACT-'+k,'family':k,'question':v['missing_test_followup'],'status':'open required acceptance mapping'} for k,v in F.items()}
SUITE_TEXT={s:(ROOT/'test'/f'{s}.test').read_text(errors='replace') for s in BASE}
ROOT_CORPUS={p.name for p in (ROOT/'test').iterdir() if p.is_file()}
BUILD_NAMES={'.editorconfig','.project','BUILD.txt','Dockerfile','Makefile.classic','Makefile.in','Makefile.osx-jaguar','auto.def','configure','.settings/org.eclipse.core.resources.prefs','.settings/org.eclipse.core.runtime.prefs'}
TOOL_ROLES={
 'co-rsync.tcl':'synchronize checkout files over rsync','codecheck1.c':'C source static code checker','decode-email.c':'decode email diagnostic utility','email-monitor.tcl':'email delivery monitoring helper','email-sender.tcl':'email sending fixture/helper','emcc.sh.in':'Emscripten build wrapper template','encode_math.sh':'math content encoding generation helper','fake-smtpd.tcl':'owned SMTP test server','find-fossil-cgis.tcl':'discover deployed Fossil CGI entries','fossil-autocomplete.bash':'Bash CLI completion','fossil-autocomplete.zsh':'Zsh CLI completion','fossil-diff-log':'diff logging wrapper','fossil-stress.tcl':'concurrent HTTP load/stress generator','fossilwiki':'wiki link/graph analysis utility','fslsrv':'multi-repository service launch/restart helper with broad process-kill behavior','makeheaders.c':'C interface header generation','makeheaders.html':'makeheaders documentation','makemake.tcl':'platform Makefile generation','man_page_command_list.tcl':'manual command list generation','mkbuiltin.c':'embedded asset source generation','mkindex.c':'command/web dispatch table generation','mkversion.c':'version metadata source generation','randomize-js-names.tcl':'JavaScript name transformation tooling','skintxt2config.c':'skin-text to repository configuration conversion','sqlcompattest.c':'external SQLite version/compile-capability configure probe','translate.c':'C source preprocessor/translation helper'}
TEST_SPECIAL={
 'tester.tcl':('inherited Tcl driver disabled by unconditional early exit','Keep pristine driver; apply separately hashed enabling/guard patches; positive/failing/missing/empty/count controls required.','adapter'),
 'settings.test.off':('dormant global/local settings and unset assertions','Not in37suite glob; enable only in isolated HOME after settings-name/list assumptions and globals review; no result exists.','dormant'),
 'release-checklist.wiki':('manual release/platform/operational checklist','Not automated suite coverage; translate each required checklist item to configuration/release acceptance evidence.','manual'),
 'update-test-1.sh':('manual delete/update/add cross-checkout scenario','Script rm-rf assumes empty cwd and prints expected observations; isolate and add machine assertions before execution.','manual'),
 'update-test-2.sh':('manual delete/update/conflict cross-checkout scenario','Script rm-rf assumes empty cwd and prints expected observations; isolate and add machine assertions before execution.','manual'),
 'many-www.tcl':('HTTP endpoint test driver','Review URL/credential/owned-server requirements and convert output checks to durable endpoint assertions; not executed.','manual'),
 'valgrind-www.tcl':('webpage memory-check driver','Requires owned server/database and valgrind-supported build/platform; not executed.','manual'),
 'link-tester.html':('browser link-test harness','Requires owned browser/server; JS/JSON companion is not an automated passed suite.','manual'),
 'link-tester.js':('browser link-test logic','Map browser route/link/crawl results to contract assertions; not executed.','manual'),
 'link-tester.json':('browser link-test input corpus','Retain exact bytes with browser link-test harness; no executed crawl result.','fixture'),
 'fake-editor.tcl':('deterministic external-editor fixture','Used by unversioned editor flow; retain Tcl invocation and input/output bytes in Rust CLI tests.','fixture'),
 'rewrite-test-output.tcl':('manual test-output rewriting tool','Retain reference only; never invoke to change expected outputs to hide failures.','manual'),
 'fileStat.th1':('TH1 file metadata fixture','Requires scripting/file platform behavior; map to TH1 and filesystem parity cases.','fixture')}
def normative(action):return 'port' if action in ['port','defer-required'] else 'replace-with-equivalent' if action=='adapt' else 'supporting-source'
def text(path):
 b=(ROOT/path).read_bytes()
 return b.decode('utf-8','replace') if b'\0' not in b[:8192] else ''
def declarations(txt):
 return [{'kind':m[1].lower(),'name':m[2],'line':txt[:m.start()].count('\n')+1} for m in re.finditer(r'^\s*\*?\*?\s*(COMMAND|WEBPAGE|SETTING):\s+([^\s]+)',txt,re.M)]
def classify(path):
 p=Path(path);name=p.name;stem=p.stem;top=p.parts[0];txt=text(path);notes=[];status='required contract mapping pending';family_key=None
 if top=='src':
  if len(p.parts)>2:
   family_key='assets' if p.suffix in ['.wav','.md'] else 'build';action='reference';role='sound/alert asset' if p.suffix=='.wav' else 'sound asset documentation/generator';dest=F[family_key]['proposed_destination']
  elif p.suffix=='.c':
   assert stem in MODULES,('unmapped C module',path)
   family_key=MODULES[stem];action=F[family_key]['disposition'];role='runtime mechanism: '+F[family_key]['required_contract'];dest=F[family_key]['proposed_destination']
  elif p.suffix in ['.js','.css','.tcl']:
   family_key='ui-workflows' if p.suffix in ['.js','.css'] else 'diff' if name=='diff.tcl' else 'merge';action='adapt';role='UI interaction/style or external visual diff/merge helper: '+name;dest='Gest UI interaction adapter' if p.suffix!='.tcl' else 'Rust CLI external visual '+stem+' adapter';notes.append('Presentation replacement does not remove the underlying action/algorithm/record contract.')
  elif p.suffix in ['.h']:
   family_key='scripting' if name.startswith('th') else 'json-api' if name=='json_detail.h' else 'build';action='adapt';role='C interface/feature/platform declarations';dest=F[family_key]['proposed_destination']
  elif name in ['Makefile','main.mk']:
   family_key='build';action='adapt';role='upstream generated/source build rules';dest='pinned C reference build and Rust Cargo/Just configuration crosswalk'
  else:
   family_key='render' if name=='markdown.md' else 'wiki-render';action='reference';role='source-adjacent format documentation/examples';dest='pinned format documentation and rendering fixture crosswalk'
 elif top=='test':
  family_key='test-surfaces';action='retain-as-test';dest='pristine inherited test/support tree';role='test fixture/support resource'
  if p.suffix=='.test':
   s=stem;role='executable Tcl suite';status='reference characterized: '+BASE[s]['state'];notes.append('Reference outcome is not Gossil coverage; framework markers are separate from actual assertions.')
   if s=='merge5':notes.append('Unconditional test_cleanup_then_return: older-than2.0 SQL dump compatibility issue; dormant/not-run despite framework success.')
  elif name in TEST_SPECIAL:
   role,note,kind=TEST_SPECIAL[name];notes.append(note);status=kind+' surface; dedicated execution/port mapping pending'
  else:notes.append('Retain exact fixture bytes; identify its feature-level consumer in addition to any delta/merge byte-corpus usage.')
 elif top=='tools':
  if path.startswith('tools/cvs2fossil/'):
   family_key='migration-tools';action='reference';role='CVS conversion '+('license/documentation' if '/doc/' in path else 'stage/library/helper '+stem);dest=F[family_key]['proposed_destination'];notes.append('Historical conversion code is accounted supporting source; supported conversion semantics remain an explicit required decision.')
  else:
   assert name in TOOL_ROLES,('unmapped tool',path)
   role=TOOL_ROLES[name];family_key='database' if name=='sqlcompattest.c' else 'service' if name in ['fake-smtpd.tcl','email-monitor.tcl','email-sender.tcl','decode-email.c'] else 'server' if name in ['fossil-stress.tcl','find-fossil-cgis.tcl','fslsrv'] else 'records' if name=='fossilwiki' else 'build';action='reference';dest='pinned utility/reference build helper; mapped Rust-equivalent contract where relevant'
   if name=='sqlcompattest.c':notes.append('Compile/run libsql-compatible capability qualification equivalent; not part of Fossil executable and not an inherited Tcl suite.')
   if name=='fslsrv':notes.append('Do not execute broad pkill/killall against user processes. Repeated service tests need owned-process rewrite.')
   if name in ['fossil-stress.tcl','fake-smtpd.tcl','email-monitor.tcl','email-sender.tcl']:notes.append('Not executed; requires bounded owned-server/process/ports and no external mail/service side effects.')
 elif top=='compat':
  family_key='dependency';action='reference';role='bundled '+('Tcl bridge declarations' if '/tcl-' in path else 'zlib compression dependency '+('test/example' if '/test/' in path or 'example' in name or 'test' in name else 'source/build/docs/license'))
  dest='pinned reference dependency tree; '+('required Tcl8.6/9 compatibility bridge' if '/tcl-' in path else 'Rust compression compatibility backend (selection pending)')
  if '/test/' in path or name.startswith('test') or 'example' in name:action='retain-as-test';notes.append('Dependency test surface retained; listing does not mean built or executed.')
 elif top=='extsrc':
  family_key='database' if name in ['sqlite3.c','sqlite3.h','shell.c'] else 'diagram' if name.startswith('pikchr') else 'dependency';action='reference';role='bundled external component '+name;dest='selected libsql family and SQL-shell adapter' if family_key=='database' else F[family_key]['proposed_destination'];notes.append('Retain exact source/license provenance; replacement preserves mapped behavioral contracts, not necessarily bundled implementation bytes.')
 elif top=='skins':
  family_key='assets';action='reference';role='upstream skin '+('documentation' if p.suffix=='.md' else 'template/style/configuration/artwork');dest='pinned skin/visual reference and Gest UI equivalence decisions'
 elif top=='www':
  family_key='docs';action='reference';role='upstream '+('artwork/document image' if p.suffix in ['.gif','.png','.svg','.eps','.jpg','.pdf'] else 'executable documentation/tool' if p.suffix in ['.tcl','.sh'] else 'notebook/example' if p.suffix=='.ipynb' else 'behavior/deployment/reference documentation');dest='pinned upstream documentation/asset evidence and contract crosswalk'
  if path.startswith('www/mdtest/') or name.startswith('title-test') or name=='uitest.html':action='retain-as-test';notes.append('Rendering/UI example surface retained; explicit assertion/browser mapping remains pending.')
  if path.startswith('www/json-api/'):notes.append('JSON protocol specification cross-reference; current C JSON-disabled build cannot qualify it.')
  if path.startswith('www/server/'):notes.append('Deployment/platform surface; embedded-server substitution needs explicit equivalent/unsupported-requirement decision, not silent omission.')
 elif top in ['win','setup']:
  family_key='build';action='reference';role='Windows '+('installer packaging' if top=='setup' else 'build/platform header/resource/deployment');dest='pinned Windows reference build/deployment evidence and Rust platform/package support decision';notes.append('Current macOS runs do not qualify Windows; retain platform contract pending explicit scope/acceptance decision.')
 elif top=='autosetup':
  family_key='build';action='reference';role='reference configure/compiler/Tcl portability dependency';dest='pinned reference build tooling; Rust Cargo/Just equivalent options'
 elif top=='.fossil-settings':
  family_key='build';action='reference';role='upstream repository versioned settings '+name;dest='source provenance/versioned-settings fixtures';notes.append('Settings bytes describe upstream checkout policy, not active Gossil gates.')
 elif path in BUILD_NAMES:
  family_key='build';action='reference';role='upstream build/editor/container configuration';dest='pinned reference build/environment evidence and Rust setup equivalent'
 elif name=='COPYRIGHT-BSD2.txt':
  family_key='dependency';action='reference';role='upstream copyright/license';dest='distributed provenance/license notices';notes.append('Preserve attribution with copied material; this inventory does not issue a new licensing judgment.')
 elif name in ['manifest','manifest.tags','manifest.uuid','VERSION']:
  family_key='build';action='reference';role='pinned source identity/version manifest';dest='reference acquisition/version verification fixtures'
 elif name in ['README.md','fossil.1']:
  family_key='docs';action='reference';role='upstream overview/manual';dest='pinned manual and public CLI contract crosswalk'
 else:raise AssertionError(('unclassified file',path))
 decl=declarations(txt)
 fam=F[family_key];suites=set(fam['candidate_inherited_suites']);static=[]
 # Explicit lexical relations are evidence of a reference, not proof that branch ran.
 for suite,body in SUITE_TEXT.items():
  if name in body or (top=='src' and any(re.search(r'(?<![\w-])'+re.escape(d['name'].rstrip('*'))+r'(?![\w-])',body) for d in decl if d['kind']=='command')):suites.add(suite);static.append(suite)
 if top=='test' and p.suffix=='.test':suites.add(stem)
 suites=sorted(s for s in suites if s in BASE)
 corpus=[]
 if top=='test' and len(p.parts)==2 and name in ROOT_CORPUS:
  corpus=['delta1']+([] if name.startswith('utf16') else ['merge2'])
 configs=[]
 for line_no,line_text in enumerate(txt.splitlines(),1):
  match=re.match(r'^\s*#\s*(if|ifdef|ifndef|elif)\b([^\n]*)',line_text)
  if match:
   value=match.group(2).strip()
   if re.search(r'WIN32|CYGWIN|__APPLE__|linux|FOSSIL_|USE_TCL|SQLITE_|NDEBUG|HAVE_',value):configs.append({'line':line_no,'condition':value})
 follow='CONTRACT-'+family_key
 return {'path':path,'source_revision':PIN,'sha256':next(x['sha256'] for x in I['files'] if x['path']==path),'surface':top if '/' in path else 'root','role':role,'disposition':normative(action),'adaptation_action':action,'proposed_destination':dest,'destination_status':'proposed logical responsibility; owning Rust module/layout not asserted','rationale':('Required engine/API behavior remains in scope under Rust implementation.' if normative(action)=='port' else 'Preserve required behavior through fixed Rust/libsql or Gest server/UI/Markdown substitution, with mapped replacement tests.' if normative(action)=='replace-with-equivalent' else 'Retain source, tests, licenses, tooling or documentation in its continuing supporting role; no runtime behavior is waived by this classification.'),'notes':notes,'status':status,'coverage':{'meaning':'contract/test relation only; no file-level behavioral proof or implementation completion','contract_family':family_key,'required_contract':fam['required_contract'],'declared_interfaces':decl,'candidate_inherited_suites':suites,'lexical_suite_references':sorted(static),'byte_corpus_suites':corpus,'reference_suite_results':[{'suite':s,'state':BASE[s]['state'],'assertions':BASE[s]['assertions'],'raw_log':BASE[s]['raw_log']} for s in suites],'missing_test_followup':follow,'full_contract_audit':'pending'},'conditional_build_platform_surfaces':configs}
rows=[classify(x['path']) for x in I['files']]
# Preserve module-specific declared interfaces and short attributed role evidence.
for row in rows:
 txt=text(row['path']);snippet=[]
 if row['path'].startswith(('src/','tools/')) and txt:
  for n,line in enumerate(txt.splitlines()[:100],1):
   if n<17:continue
   cleaned=re.sub(r'^[\s*#]+','',line).strip()
   if cleaned and not cleaned.startswith(('#include','include ','Copyright','This program is','modify it','known as','MERCHANTABILITY','FITNESS','Author contact','drh@','http://')):
    snippet.append({'line':n,'text':cleaned})
   if len(snippet)>=8:break
 row['source_role_evidence']={'kind':'attributed source-header excerpt; navigation evidence, not behavioral audit','source_revision':PIN,'path':row['path'],'lines':snippet}
BY_PATH={x['path']:x for x in rows}
# Account every diagnostic declaration and every installed hook separately.
hooks=[]
for decl in I['diagnostic_command_declarations']:
 name=decl['name'];canonical=name.rstrip('*');row=BY_PATH[decl['path']];txt=text(decl['path']);lines=txt.splitlines();description=[]
 for line in lines[decl['line']:decl['line']+35]:
  if '*/' in line:break
  clean=re.sub(r'^[\s*]+','',line).strip()
  if clean:description.append(clean)
 lexical=[s for s,b in SUITE_TEXT.items() if re.search(r'(?<![\w-])'+re.escape(canonical)+r'(?![\w-])',b)]
 observed=[]
 for suite,record in BASE.items():
  log=Path(record['raw_log']).read_text(errors='replace')
  hits=[{'line':i,'command':v} for i,v in enumerate(log.splitlines(),1) if re.search(r'/fossil\s+'+re.escape(canonical)+r'(?:\s|$)',v)]
  if hits:observed.append({'suite':suite,'reference_suite_state':record['state'],'invocation_count':len(hits),'log':record['raw_log'],'first_invocation':hits[0],'meaning':'command invocation logged; not proof every diagnostic branch/assertion passed'})
 available=canonical in CAP['native_test_command_names']
 reasons={'test-is-normal-dir':'Windows-only source block; absent on macOS','test-hash-from-path':'source #if 0 dormant block; * marks declaration','test-th-hook':'FOSSIL_ENABLE_TH1_HOOKS disabled in selected reference build'}
 hooks.append({'declaration_name':name,'canonical_name':canonical,'source_revision':PIN,'source_path':decl['path'],'source_sha256':row['sha256'],'source_line':decl['line'],'source_help_excerpt':description,'disposition':row['disposition'],'adaptation_action':'preserve diagnostic interface and exercise corresponding Rust mechanism; no C delegation for Gossil results','proposed_destination':row['proposed_destination']+' / compatibility test diagnostic entry point','required_contract_family':row['coverage']['contract_family'],'runtime_availability':'listed' if available else 'not-listed','availability_evidence':'/tmp/gossil-fossil-baseline-20261117/capability-inventory.json','unavailable_reason':None if available else reasons.get(canonical,'unresolved availability mismatch'),'lexical_inherited_suite_references':lexical,'observed_reference_invocations':observed,'gossil_implementation_status':'not established by this inventory','missing_test_followup':{'id':'HOOK-'+canonical,'required_action':'Derive focused contract cases from this exact diagnostic implementation/help, including input/exit/error/state boundaries; map each invoking suite to Rust diagnostic without invoking C.','status':'pending'}})
runtime_hooks=[]
for name in CAP['native_test_command_names']:
 matches=[x for x in hooks if x['canonical_name']==name]
 runtime_hooks.append({'name':name,'availability':'listed, not necessarily executed','source_mapping':[{'path':x['source_path'],'line':x['source_line'],'sha256':x['source_sha256'],'canonical_name':x['canonical_name']} for x in matches],'hook_contract_ids':['HOOK-'+x['canonical_name'] for x in matches]})
# Explicit source/test/build/platform matrix, not a claimed qualified composition.
configurations=[
 {'id':'REF-MAC-ARM64-MIN','scope':'observed selected reference','conditions':['macOS arm64','Tcl harness8.6.18','Fossil2.29 7e8ecf0ce4','TLS/FUSE disabled','JSON/Tcl integration/TH1 docs/hooks disabled'],'evidence':'/tmp/gossil-fossil-baseline-20261117/baseline-240s.json','qualification':'characterized partial outcomes:25pass/5fail/4feature-disabled/1dormant/2timeout, not complete acceptance'},
 {'id':'TLS','scope':'required feature configuration','conditions':['FOSSIL_ENABLE_SSL','certificate/trust/proxy/client/server failure cases'],'status':'not-run; selected build disabled','families':['sync','server']},
 {'id':'FUSE','scope':'required feature/platform configuration','conditions':['FUSE-enabled supported OS','mount/unmount/read-only view and permissions'],'status':'not-run; selected build disabled','families':['platform-fuse']},
 {'id':'JSON','scope':'required feature configuration','conditions':['FOSSIL_ENABLE_JSON','Tcl json/json::write packages'],'status':'feature-disabled in selected baseline','families':['json-api','database']},
 {'id':'TH1-DOCS-HOOKS','scope':'required feature configurations','conditions':['FOSSIL_ENABLE_TH1_DOCS','FOSSIL_ENABLE_TH1_HOOKS','enabled and disabled runtime behavior'],'status':'enabled branches not-run; selected baseline disabled','families':['scripting','server']},
 {'id':'TCL-BRIDGE','scope':'required scripting ABI configurations','conditions':['FOSSIL_ENABLE_TCL','Tcl8.6 and9 headers/runtime','USE_TCL_STUBS/FOSSIL_ENABLE_TCL_STUBS/FOSSIL_ENABLE_TCL_PRIVATE_STUBS where supported'],'status':'not-run; selected reference Tcl integration disabled','families':['scripting','dependency']},
 {'id':'DELTA-CKSUM','scope':'required conditional algorithm configuration','conditions':['FOSSIL_ENABLE_DELTA_CKSUM_TEST enabled and disabled'],'status':'finite default-build corpus observed checksums not enforced; enabled build not-run','families':['bytes']},
 {'id':'FUZZ-SANITIZERS','scope':'required fuzz/diagnostic build configuration','conditions':['FOSSIL_FUZZ','source FUZZ_* selectors','Linux/libFuzzer with address/undefined sanitizers and mapped Rust fuzz targets'],'status':'entrypoints inspected only; no campaign run','families':['test-infrastructure']},
 {'id':'PLATFORM-POSIX','scope':'required platform qualification decision/matrix','conditions':['macOS/Linux','arm64/x86_64','filesystem case/Unicode/symlink/executable bits','process/socket/terminal behaviors'],'status':'only selected macOSarm64 baseline characterized; remaining combinations not-run','families':['checkout','process','sync','server']},
 {'id':'PLATFORM-WINDOWS-CYGWIN','scope':'retained upstream platform contracts; support decision required','conditions':['Windows file attributes/reserved names/service/process/socket behavior','Cygwin path/platform branches','Windows-only test-is-normal-dir'],'status':'not-run; no exclusion inferred from current host or reduced UI','families':['platform-windows','checkout','process']},
 {'id':'SQLITE-LIBSQL','scope':'required database-equivalence qualification','conditions':['tools/sqlcompattest.c minimum version and compile features','bundled versus external SQLite reference variants','selected libsql family','functions/virtual tables/FTS/schema/locking/WAL/transactions/extensions'],'status':'source relationship only; libsql equivalence qualification pending','families':['database','dependency']},
 {'id':'MANUAL-RELEASE-DEPLOY','scope':'required manual/deployment surface qualification','conditions':['test/release-checklist.wiki','test/update-test-1.sh','test/update-test-2.sh','CGI/SCGI/services/proxies/installers/browser/link/memory/load/mail scripts'],'status':'not-run; isolate resources and convert observations to assertions before execution','families':['build','test-surfaces','server','service']}
]
for family_key,q in QUESTIONS.items():q['files']=[r['path'] for r in rows if r['coverage']['contract_family']==family_key]
summary={'status':'exhaustive file/disposition/header accounting; behavioral contract audits and implementation remain pending','phase':'phase1 inventory component only; not a phase1 completion declaration','source_revision':PIN,'native_revision':B['provenance']['native_pin'],'normative_disposition_enum':['port','replace-with-equivalent','supporting-source'],'coverage_definition':{'complete':'Every tracked file has exact identity/hash, role, normative disposition, proposed responsibility, rationale and linked test/contract relation or missing-test follow-up; every declared/native diagnostic hook maps to pinned source.','not_proven':['file-by-file behavioral correctness','complete function/branch/API/SQL schema inventory','complete assertion-to-contract mapping','Rust implementation or Gossil behavior','all reference suites passing','all build/platform configurations qualified','phase1 gate complete']},'fixed_substitutions':['Rust implementation language/runtime','selected libsql family for SQLite/database integration','Gest-derived embedded server','Gest UI','Gest Markdown engine'],'engine_scope_rule':'All repository, sync, scripting, rendering/wiki/diagram, administration, alerts/chat/extensions, auth, hashing and platform mechanisms remain required unless an explicit requirement maps a behavioral replacement; UI reduction cannot drop an engine contract.','files':rows,'declared_diagnostic_hooks':hooks,'runtime_diagnostic_hooks':runtime_hooks,'contract_families':F,'required_contract_questions':list(QUESTIONS.values()),'configuration_matrix':configurations,'existing_reference_baseline':{'report':'/tmp/gossil-fossil-baseline-20261117/baseline-240s.json','states':B['states'],'actual_assertions':B['assertion_totals'],'framework':B['framework_totals'],'supplemental_delta':'/tmp/gossil-fossil-baseline-20261117/supplemental-delta.json','meaning':'characterized reference outcomes; disabled/dormant/timeout/failures remain visible'},'related_finite_delta_corpus':{'manifest':'/tmp/gossil-delta-decoder-corpus-20261117/manifest.json','meaning':'59executed finite decoder cases only;5large headers not-run; not full delta conformance'},'recommendation':'Review open contract questions and normative dispositions independently, then close remaining phase1 acceptance prerequisites. Advance engine phase2 only after the director records the phase1 decision.'}
(R/'inventory.json').write_text(json.dumps(summary,indent=2))
(R/'short-report.json').write_text(json.dumps({'source_revision':PIN,'file_count':len(rows),'surfaces':dict(collections.Counter(r['surface'] for r in rows)),'dispositions':dict(collections.Counter(r['disposition'] for r in rows)),'declared_hook_count':len(hooks),'runtime_hook_count':len(runtime_hooks),'unavailable_hooks':[{k:x[k] for k in ['canonical_name','source_path','source_line','unavailable_reason']} for x in hooks if x['runtime_availability']=='not-listed'],'questions':[{k:q[k] for k in ['id','question','status']} for q in QUESTIONS.values()],'reference_states':B['states'],'completion_boundary':summary['coverage_definition'],'phase1_complete_claim':False,'recommendation':summary['recommendation']},indent=2))
print(json.dumps({'files':len(rows),'src':sum(r['surface']=='src' for r in rows),'test':sum(r['surface']=='test' for r in rows),'tools':sum(r['surface']=='tools' for r in rows),'hooks':len(hooks),'runtime':len(runtime_hooks)},indent=2))
