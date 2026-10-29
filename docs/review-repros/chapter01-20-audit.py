#!/usr/bin/env python3
"""1~20장 명령과 빠진 공통 쓰기 걸음을 임시 vault/serve로 재현한다. 엔진 소스는 수정하지 않는다."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time
import uuid
import zipfile

ROOT_ID = "ndfixture-00000001"
SUB_ID = "snfixture-00000001"
COMMENT_ID = "cmfixture-0000000a"
AGENDA_ID = "agfixture-00000001"
EMPTY_AGENDA_ID = "agfixture-00000002"


def source_signature(root):
    digest = hashlib.sha256()
    files = sorted((root / "src").rglob("*.java"))
    for file in files:
        digest.update(str(file.relative_to(root)).encode() + b"\0" + file.read_bytes() + b"\0")
    return {"files": len(files), "sha256": digest.hexdigest()}


def run(argv, env, cwd, timeout=15):
    return subprocess.run(argv, cwd=cwd, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout)


def write(root, relative, text):
    file = root / relative
    file.parent.mkdir(parents=True, exist_ok=True)
    file.write_text(text, encoding="utf-8")


def fixture(root):
    write(root, ".withlog/workspace.json", '{"schemaVersion":1}\n')
    write(root, ".withlog/agendas/demo.md", f"---\nid: {AGENDA_ID}\nslug: demo\ntitle: 데모\nhue: blue\n---\n")
    write(root, ".withlog/agendas/empty.md", f"---\nid: {EMPTY_AGENDA_ID}\nslug: empty\ntitle: 비었음\nhue: green\n---\n")
    def note(note_id, title, body):
        return (f"---\nid: {note_id}\nagenda: demo\ntype: free\ntitle: {title}\n"
                "created: '2026-06-16T10:00:00+09:00'\nmins: 30\ntags: [검사]\n---\n" + body)
    write(root, "notes/demo/root/note.md", note(ROOT_ID, "검사 노트", f"# 검사 노트\n\n본문입니다.\n[자식](#sub:{SUB_ID})\n"))
    write(root, "notes/demo/root/sub/child/note.md", note(SUB_ID, "검사 하위", "# 검사 하위\n\n본문입니다.\n"))
    write(root, "notes/demo/root/comments.json", json.dumps({"v": 1, "comments": [
        {"id": COMMENT_ID, "createdAt": "2026-06-16T10:30:00.000Z", "body": "기존 코멘트"}
    ]}, ensure_ascii=False) + "\n")


def token(file):
    return "f1:" + hashlib.sha256(file.read_bytes()).hexdigest()


def response(process):
    stdout = process.stdout.decode("utf-8", errors="replace")
    stderr = process.stderr.decode("utf-8", errors="replace")
    match = re.search(r"\bWL-(\d{3})\b", stderr)
    try:
        parsed = json.loads(stdout)
    except ValueError:
        parsed = None
    return {"exitCode": process.returncode, "wlCode": int(match[1]) if match else None,
            "stdoutBytes": len(process.stdout), "stderr": stderr, "data": parsed}


def smoke_cases(vault):
    root_token = token(vault / "notes/demo/root/note.md")
    sub_token = token(vault / "notes/demo/root/sub/child/note.md")
    return [
        ["reindex"], ["reindex-paths", "notes/demo/root"],
        ["list-docs"], ["search-docs", "본문"], ["get-doc", ROOT_ID], ["get-sub-note", ROOT_ID, SUB_ID], ["list-tags"],
        ["create-doc", "agenda=demo", "type=free", "title=새 노트", "body=새 본문"],
        ["update-doc", ROOT_ID, "body=고친 본문", f"f1={root_token}"],
        ["set-favorite", ROOT_ID, "favorite=true", f"f1={root_token}"],
        ["evaluate-doc", ROOT_ID, "score=80", f"f1={root_token}"],
        ["link-docs", ROOT_ID, "ndfixture-00000002", f"f1={root_token}"],
        ["list-sub-notes", ROOT_ID],
        ["create-sub-note", ROOT_ID, "title=새 하위", "body=하위 본문"],
        ["update-sub-note", ROOT_ID, SUB_ID, "body=# 고친 하위\n\n본문", f"f1={sub_token}"],
        ["delete-sub-note", ROOT_ID, SUB_ID], ["list-attachments", ROOT_ID],
        ["write-attachment", ROOT_ID, "name=검사.txt", "content=파일 본문"],
        ["list-comments", ROOT_ID], ["create-comment", ROOT_ID, "body=새 코멘트"],
        ["update-comment", ROOT_ID, f"comment_id={COMMENT_ID}", "body=고친 코멘트"],
        ["delete-comment", ROOT_ID, f"comment_id={COMMENT_ID}"],
        ["list-agendas"], ["create-agenda", "title=새 아젠다"],
        ["update-agenda", AGENDA_ID, "title=고친 아젠다"], ["delete-agenda", EMPTY_AGENDA_ID],
    ]


def audit(root, java):
    jar = root / "dist/wl-engine.jar"
    cli = root / "dist/wl-cli.jar"
    if not jar.is_file() or not cli.is_file():
        raise RuntimeError("엔진/CLI JAR을 먼저 ./build.sh로 빌드한다.")
    report = {"source": source_signature(root), "smoke": []}
    with tempfile.TemporaryDirectory(prefix="aisurada-audit-probes-") as temporary:
        base = Path(temporary)
        home = base / "home"
        home.mkdir()
        env = {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin", "HOME": str(home), "TMPDIR": str(base),
               "TZ": "Asia/Seoul", "GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": "/dev/null"}
        initial = base / "initial"
        fixture(initial)
        for number, args in enumerate(smoke_cases(initial)):
            vault = base / f"case-{number:02d}"
            shutil.copytree(initial, vault)
            process = run([java, f"-Duser.home={home}", "-jar", str(jar), *args, f"--withlog.vault={vault}"], env, base)
            result = response(process)
            report["smoke"].append({"command": args[0], "expectedExitCode": 0, **result})
        report["smokeSummary"] = {"total": len(report["smoke"]),
            "success": sum(item["exitCode"] == 0 for item in report["smoke"]),
            "unsupported": [item["command"] for item in report["smoke"] if item["wlCode"] == 409]}

        # 3.2/7.1의 startup, writer 전환, idle, shutdown은 같은 serve 인스턴스에서 재야 한다.
        vault = base / "git-vault"
        shutil.copytree(initial, vault)
        git = shutil.which("git")
        def git_run(*args):
            result = run([git, "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false", *args], env, vault)
            if result.returncode:
                raise RuntimeError(result.stderr.decode(errors="replace"))
            return result.stdout.decode().strip()
        git_run("init", "-b", "main")
        git_run("config", "user.name", "Audit Fixture")
        git_run("config", "user.email", "audit@example.invalid")
        git_run("add", "-A")
        git_run("commit", "-m", "baseline fixture")
        body_file = vault / "notes/demo/root/note.md"
        body_file.write_text(body_file.read_text() + "\n외부 편집\n")
        counts = {"beforeStartup": int(git_run("rev-list", "--count", "HEAD"))}
        socket = Path("/tmp") / ("asa-" + uuid.uuid4().hex[:10] + ".sock")
        serve_env = {**env, "WITHLOG_VAULT": str(vault), "WITHLOG_SIDECAR_SOCKET": str(socket),
                     "WITHLOG_VAULT_AUTOCOMMIT_ENABLED": "true", "WITHLOG_VAULT_AUTOCOMMIT_IDLE_SECONDS": "0"}
        command_results = []
        with (base / "serve.log").open("wb") as log:
            server = subprocess.Popen([java, f"-Duser.home={home}", "-jar", str(jar), "serve"],
                                      env=serve_env, cwd=base, stdout=log, stderr=log)
            try:
                deadline = time.monotonic() + 10
                while not socket.exists() and server.poll() is None and time.monotonic() < deadline:
                    time.sleep(0.025)
                if not socket.exists():
                    raise RuntimeError("serve 소켓을 열지 못했다")
                counts["afterStartup"] = int(git_run("rev-list", "--count", "HEAD"))
                def cli_run(*args):
                    result = response(run([java, f"-Duser.home={home}", "-Dwithlog.launchctl.cmd=/usr/bin/false",
                        "-jar", str(cli), *args], serve_env, base))
                    command_results.append({"command": args[0], **result})
                    if result["exitCode"] != 0:
                        raise RuntimeError(json.dumps(result, ensure_ascii=False))
                    return result
                cli_run("set_favorite", json.dumps({"id": ROOT_ID, "favorite": True, "f1": token(body_file)}))
                counts["afterToolWrite"] = int(git_run("rev-list", "--count", "HEAD"))
                cli_run("set-favorite", ROOT_ID, "favorite=false", "f1=" + token(body_file))
                counts["afterWriterChange"] = int(git_run("rev-list", "--count", "HEAD"))
                time.sleep(5.5)  # serve 정비 5초를 한 번 지나 idle 커밋을 확인한다.
                counts["afterMaintenance"] = int(git_run("rev-list", "--count", "HEAD"))
                cli_run("set-favorite", ROOT_ID, "favorite=true", "f1=" + token(body_file))
                counts["beforeShutdown"] = int(git_run("rev-list", "--count", "HEAD"))
            finally:
                server.terminate()
                try:
                    server.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    server.kill()
                    server.wait(timeout=5)
                socket.unlink(missing_ok=True)
        counts["afterShutdown"] = int(git_run("rev-list", "--count", "HEAD"))
        report["automaticCommit"] = {"enabled": True, "idleSeconds": 0, "branch": git_run("branch", "--show-current"),
            "counts": counts, "writes": command_results, "finalStatus": git_run("status", "--porcelain")}
        with zipfile.ZipFile(jar) as archive:
            names = set(archive.namelist())
            report["optionalBuildDiagnostics"] = {
                "hasCommitStamp": "META-INF/wl-build-commit.txt" in names,
                "hasWatchedPathsStamp": "META-INF/wl-watched-paths.txt" in names,
                "note": "3.6은 재구현 doctor의 이런 진단을 바꾸거나 생략해도 된다고 명시한다."}
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--java", default=shutil.which("java"))
    parser.add_argument("--output", type=Path)
    options = parser.parse_args()
    result = audit(options.root.resolve(), str(Path(options.java).resolve()))
    output = json.dumps(result, ensure_ascii=False, indent=2) + "\n"
    if options.output:
        options.output.write_text(output, encoding="utf-8")
        print(json.dumps({"smoke": result["smokeSummary"], "automaticCommit": result["automaticCommit"]["counts"]}, ensure_ascii=False))
    else:
        print(output, end="")
