#!/usr/bin/env python3
"""`commit-vault`를 같은 git 저장소에 기준 Java 엔진과 Rust `ais-cli`로 돌려 견준다(이슈 #81, 지운 `VaultGitTest`의 명령줄에서 보이는 부분).

가짜 시계·가짜 git 실행기를 끼우는 Java 단위 시험은 그대로 옮길 수 없어서, 같은 동작을 실제 저장소에서 본다:
훅과 서명 무시, 작성자, 커밋 메시지 바이트, `.tmp` 제외, 잠금 파일 나이(599초와 601초), 진행 중 표지 다섯, 브랜치, 끔·저장소 아님,
노트 id 중복 제거와 상한 200, 머리 id 모양. 응답은 JSON으로 읽어 키 순서를 무시하고 견준다.

사용: AISURADA_ORACLE_DIR=<기준 jar 폴더> AIS_CLI=<Rust ais-cli> python3 commit_vault_twin.py
"""
import os
import tempfile

import subprocess, shutil, sys, time, json
S=tempfile.mkdtemp(prefix='ais-gittwin-'); ORACLE=os.environ['AISURADA_ORACLE_DIR']+'/ais-engine.jar'; RUST=os.environ['AIS_CLI']
GENV=dict(os.environ, GIT_AUTHOR_NAME='Fixture', GIT_AUTHOR_EMAIL='f@example.invalid', GIT_COMMITTER_NAME='Fixture', GIT_COMMITTER_EMAIL='f@example.invalid', GIT_AUTHOR_DATE='2026-10-29T10:00:00+0900', GIT_COMMITTER_DATE='2026-10-29T10:00:00+0900')
def git(v,*a,check=True):
    r=subprocess.run(['git','-c','core.hooksPath=/dev/null','-c','commit.gpgsign=false']+list(a),cwd=v,env=GENV,capture_output=True,text=True)
    if check and r.returncode: raise Exception(r.stderr)
    return r.stdout
def write(v,rel,text):
    p=os.path.join(v,rel); os.makedirs(os.path.dirname(p),exist_ok=True); open(p,'w').write(text)
def note(i,body): return f"---\nid: {i}\nagenda: demo\ntitle: 검사\ntype: free\ncreated: '2026-06-16T10:00:00+09:00'\n---\n{body}\n"
ID='ndfixture-00000001'
def repo(root,name):
    v=f'{root}/{name}'; shutil.rmtree(v,ignore_errors=True); os.makedirs(v)
    git(v,'init','-q','-b','main'); write(v,'.aisurada/workspace.json','{}'); write(v,'.aisurada/agendas/demo.md','---\nid: agfixture-00000001\nslug: demo\ntitle: 데모\n---\n'); write(v,'notes/demo/root/note.md',note(ID,'본문')); git(v,'add','-A'); git(v,'commit','-qm','baseline'); return v
def run(engine,v,root,*args):
    home=f'{root}/home'; os.makedirs(home,exist_ok=True)
    env={'PATH':os.environ['PATH'],'HOME':home,'AISURADA_VAULT':v,'AISURADA_LOCK_DIR':f'{root}/locks','TZ':'Asia/Seoul'}
    if engine=='java': cmd=['java',f'-Duser.home={home}','-jar',ORACLE,*args,f'--aisurada.vault={v}']
    else: cmd=[RUST,*args]
    r=subprocess.run(cmd,env=env,capture_output=True,text=True)
    out=r.stdout.strip()
    try: out=json.loads(out)
    except Exception: pass
    return r.returncode,out,r.stderr.strip().split('\n')[0][:60]
def msg(v): return git(v,'show','-s','--format=%B','HEAD').rstrip()+'\n'
def who(v): return git(v,'show','-s','--format=%an <%ae>|%cn <%ce>','HEAD').strip()
def count(v): return int(git(v,'rev-list','--count','HEAD').strip())
results={}
def scenario(name,fn):
    out={}
    for eng in ('java','rust'):
        root=f'{S}/gt-{eng}'; shutil.rmtree(root,ignore_errors=True); os.makedirs(root)
        try: out[eng]=fn(eng,root)
        except Exception as e: out[eng]=f'EXC {e}'
    same = out['java']==out['rust']
    print(('SAME ' if same else 'DIFF ')+name)
    if not same:
        print('   java:',json.dumps(out['java'],ensure_ascii=False)[:600]); print('   rust:',json.dumps(out['rust'],ensure_ascii=False)[:600])

def s_commit(eng,root):
    v=repo(root,'v')
    write(v,'.git/hooks/post-commit','#!/bin/sh\ntouch hook-ran\n'); os.chmod(v+'/.git/hooks/post-commit',0o755)
    git(v,'config','commit.gpgsign','true'); git(v,'config','gpg.program','/usr/bin/false')
    write(v,'notes/demo/root/note.md',note(ID,'외부 편집')); write(v,'a/deep/half.tmp','temporary')
    r=run(eng,v,root,'commit-vault')
    return [r,os.path.exists(v+'/hook-ran'),msg(v),who(v),'a/' in git(v,'status','--porcelain'),count(v)]
scenario('외부 편집 커밋: 훅 끔, 서명 끔, 작성자, 메시지, .tmp 제외',s_commit)

def s_lock(eng,root):
    v=repo(root,'v'); write(v,'changed','x'); res=[]
    lock=v+'/.git/index.lock'
    for age in (0,-599,601):  # fresh, 599 s old, 601 s old
        write(v,'.git/index.lock','')
        t=time.time()-max(age,0) if age>=0 else time.time()+age
        t=time.time()-(abs(age) if age!=0 else 0)
        os.utime(lock,(t,t))
        res.append([age,run(eng,v,root,'commit-vault'),os.path.exists(lock),count(v)])
    return res
scenario('index.lock 나이(신선/599초/601초)',s_lock)

def s_progress(eng,root):
    v=repo(root,'v'); write(v,'changed','x'); res=[]
    for marker in ['rebase-merge','rebase-apply','MERGE_HEAD','CHERRY_PICK_HEAD','REVERT_HEAD']:
        p=f'{v}/.git/{marker}'
        if marker.startswith('rebase'): os.makedirs(p,exist_ok=True)
        else: write(v,f'.git/{marker}','')
        res.append([marker,run(eng,v,root,'commit-vault')])
        (shutil.rmtree(p) if os.path.isdir(p) else os.remove(p))
    return res
scenario('진행 중 표지 다섯',s_progress)

def s_branch(eng,root):
    v=repo(root,'v'); write(v,'changed','x'); git(v,'checkout','-qb','other'); a=run(eng,v,root,'commit-vault'); git(v,'checkout','-q','--detach'); b=run(eng,v,root,'commit-vault'); return [a,b]
scenario('다른 브랜치와 detached',s_branch)

def s_off(eng,root):
    v=repo(root,'v'); write(v,'changed','x'); os.environ['AISURADA_VAULT_AUTOCOMMIT_ENABLED']='false'
    try:
        home=f'{root}/home'; os.makedirs(home,exist_ok=True)
        env={'PATH':os.environ['PATH'],'HOME':home,'AISURADA_VAULT':v,'AISURADA_LOCK_DIR':f'{root}/locks','AISURADA_VAULT_AUTOCOMMIT_ENABLED':'false'}
        cmd=['java',f'-Duser.home={home}','-jar',ORACLE,'commit-vault',f'--aisurada.vault={v}','--aisurada.vault.autocommit.enabled=false'] if eng=='java' else [RUST,'commit-vault']
        r=subprocess.run(cmd,env=env,capture_output=True,text=True)
    finally: del os.environ['AISURADA_VAULT_AUTOCOMMIT_ENABLED']
    nr=f'{root}/nr'; os.makedirs(nr+'/.aisurada'); open(nr+'/.aisurada/workspace.json','w').write('{}'); r2=run(eng,nr,root,'commit-vault')
    os.makedirs(f'{root}/nr2/.aisurada'); open(f'{root}/nr2/.aisurada/workspace.json','w').write('{}'); open(f'{root}/nr2/.git','w').write('gitdir: elsewhere'); r3=run(eng,f'{root}/nr2',root,'commit-vault')
    return [r.returncode,json.loads(r.stdout),r2,r3]
scenario('끔(off)과 저장소 아님',s_off)

def s_ids(eng,root):
    v=repo(root,'v'); os.remove(v+'/notes/demo/root/note.md')
    write(v,'notes/demo/renamed/note.md',note(ID,'같은 id 이동')); write(v,'notes/demo/bom/note.md','﻿'+note('bom','x')); write(v,'notes/demo/json/note.json',note('json','x'))
    a=run(eng,v,root,'commit-vault'); m1=msg(v)
    for i in range(201): write(v,f'bulk/{i}/note.md',f'---\nid: note_{i}\n---\n')
    b=run(eng,v,root,'commit-vault'); m2=msg(v)
    return [a,m1,b,m2.split('\n')[0],len(m2.split('Aisurada-Notes: ')[1].split())]
scenario('노트 id 중복 제거·BOM/json 제외, 201개와 꼬리 200개 상한',s_ids)

def s_headers(eng,root):
    v=repo(root,'v'); os.remove(v+'/notes/demo/root/note.md')
    variants={'good':"---\nid: good\n---\n", 'quoted':"---\nid: '\"한글_1-\"'\n---\n", 'bom':"﻿---\nid: bomid\n---\n", 'indent':"---\n id: indentid\n---\n", 'space':"---\nid: x y\n---\n", 'noclose':"---\nid: noclose\n", 'bang':"---\nid: bad!\nid: good2\n---\n", 'crlf':"---\r\nid: crlfid\r\n---\r\n", 'dq':'---\nid: "dq-id"\n---\n', 'long':"---\nid: "+"a"*200+"\n---\n", 'under':"---\nid: _lead\n---\n", 'dot':"---\nid: a.b\n---\n"}
    for k,t in variants.items(): write(v,f'notes/h/{k}/note.md',t)
    r=run(eng,v,root,'commit-vault'); m=msg(v)
    trailer=m.split('Aisurada-Notes: ')[1].split() if 'Aisurada-Notes: ' in m else []
    return [r,m.split('\n')[0],sorted(trailer)]
scenario('머리 id 모양(받는 것과 버리는 것)',s_headers)

shutil.rmtree(S, ignore_errors=True)
