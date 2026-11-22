#!/usr/bin/env python3
"""Capture the diagnostics of the graph algorithms. The fabricated indexes confer no admission."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import sqlite3
import subprocess
import tempfile
import zlib

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--fossil', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
fossil = args.fossil.resolve(strict=True)
nodes = [dict(rid=i, branch=('feature' if i in (4, 6) else 'other' if i in (7, 8) else 'trunk'), mtime=2451545.0+i) for i in range(1, 13)]
links = [dict(parent=p, child=c, primary=bool(v)) for p,c,v in sorted([(1,2,1),(2,3,1),(2,4,1),(3,5,1),(4,5,0),(4,6,1),(1,7,1),(7,8,1),(10,11,1),(11,12,1),(8,12,0)])]
cherries = [dict(parent=6, child=8, excluded=False),dict(parent=6,child=9,excluded=True)]
cases = []
for a,b,flags in [(1,5,[]),(5,6,[]),(5,6,['--no-merge']),(3,8,[]),(8,3,[]),(5,6,['--one-way']),(1,5,['--one-way','--no-merge']),(6,8,[]),(9,1,[]),(1,1,[]),(5,6,['--branch-cost','5']),(5,6,['--branch-cost','100']),(1,12,[]),(12,3,[]),(3,12,['--one-way']),(5,6,['--debug'])]:
    cases.append(dict(kind='shortest', start=a, end=b, flags=flags))
for a,b in [(3,4),(5,6),(3,8),(8,3),(12,5),(9,1),(1,1),(5,4),(4,5)]:
    cases.append(dict(kind='ancestor',start=a,end=b,flags=[]))
with tempfile.TemporaryDirectory(prefix='gossil-native-history-') as temp:
    temp = Path(temp)
    home = temp/'home'; home.mkdir()
    env = dict(os.environ, HOME=str(home), FOSSIL_HOME=str(home), LC_ALL='C', TZ='UTC')
    commands=[]
    def run(argv):
        result=subprocess.run([str(fossil),*argv],cwd=temp,env=env,capture_output=True,text=True,timeout=20)
        commands.append(dict(argv=argv,returncode=result.returncode,stdout=result.stdout,stderr=result.stderr))
        return result
    version=run(['version'])
    result=run(['new','repo.fossil','--admin-user','history-fixture'])
    if result.returncode: raise RuntimeError(result.stderr)
    db=sqlite3.connect(temp/'repo.fossil')
    # The native repository is deliberately a synthetic diagnostic fixture,
    # not an admitted Gossil repository or artifact-crosslinking experiment.
    for table in ('plink','cherrypick','event','tagxref','blob'):
        db.execute('DELETE FROM '+table)
    hashes={}
    for n in nodes:
        rid=n['rid']; hashes[rid]=hashlib.sha1(('history-node-'+str(rid)).encode()).hexdigest()
        db.execute('INSERT INTO blob(rid,uuid,size,content) VALUES(?,?,0,?)',(rid,hashes[rid],bytes(4)+zlib.compress(b'')))
        db.execute("INSERT INTO event(type,mtime,objid) VALUES('ci',?,?)",(n['mtime'],rid))
        db.execute('INSERT INTO tagxref(tagid,tagtype,srcid,origid,value,mtime,rid) VALUES(8,2,0,0,?,?,?)',(n['branch'],n['mtime'],rid))
    for l in links:
        db.execute('INSERT INTO plink(pid,cid,isprim,mtime) VALUES(?,?,?,?)',(l['parent'],l['child'],int(l['primary']),nodes[l['child']-1]['mtime']))
    for l in cherries:
        db.execute('INSERT INTO cherrypick(parentid,childid,isExclude) VALUES(?,?,?)',(l['parent'],l['child'],int(l['excluded'])))
    db.commit()
    descriptions={db.execute("SELECT strftime('%Y%m%d%H%M',?)",(n['mtime'],)).fetchone()[0]+'/'+n['branch']:n['rid'] for n in nodes}
    query_rows={str(n['rid']):dict(children=db.execute('SELECT cid,isprim FROM plink WHERE pid=?',(n['rid'],)).fetchall(),parents=db.execute('SELECT pid,isprim FROM plink WHERE cid=?',(n['rid'],)).fetchall()) for n in nodes}
    for c in cases:
        result=run(['test-shortest-path' if c['kind']=='shortest' else 'test-ancestor-path','-R','repo.fossil',*c['flags'],hashes[c['start']],hashes[c['end']]])
        c.update(returncode=result.returncode,stdout=result.stdout,stderr=result.stderr)
        if c['kind']=='shortest':
            c['path']=[descriptions[m.group(1)] for line in result.stdout.splitlines() if (m:=re.fullmatch(r'\s*\d+: (\S+)',line))]
        else:
            c['path']=[int(m.group(1)) for line in result.stdout.splitlines() if (m:=re.match(r'\s*\d+:\s+(\d+) ',line))]
            c['pivot']=next((int(m.group(1)) for line in result.stdout.splitlines() if ' PIVOT' in line and (m:=re.match(r'\s*\d+:\s+(\d+) ',line))),None)
    payload=dict(format=1,scope='explicit synthetic graph algorithm only; no persisted Gossil admission',native_version=version.stdout.strip(),binary_sha256=hashlib.sha256(fossil.read_bytes()).hexdigest(),nodes=nodes,links=links,cherrypicks=cherries,query_rows=query_rows,cases=cases,commands=commands)
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(payload,indent=2)+'\n')
print(f'Captured {len(cases)} isolated native diagnostic cases to {args.output}')
