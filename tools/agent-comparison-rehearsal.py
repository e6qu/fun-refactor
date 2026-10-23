#!/usr/bin/env python3
"""GitHub-only, network-free tool and detached-process accounting rehearsal."""
import argparse
from contextlib import contextmanager
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import uuid

from agent_eval.container_resources import ContainerResources, audit, limits, unit
from agent_eval.isolated_grade import DOCKER, invoke
from agent_eval.study import require


@contextmanager
def isolated_budget(directory, budget):
    name = "frstudy" + uuid.uuid4().hex + ".slice"
    local = directory / name
    local.write_text(unit(name, budget))
    destination = "/run/systemd/system/" + name

    def administrative(*argv):
        subprocess.run(argv, check=True, capture_output=True, timeout=15)

    try:
        administrative("sudo", "install", "-m", "0644", str(local), destination)
        administrative("sudo", "systemctl", "daemon-reload")
        administrative("sudo", "systemctl", "start", name)
        administrative("sudo", "chown", str(os.getuid()), "/sys/fs/cgroup/" + name + "/cgroup.kill")
        yield name
    finally:
        administrative("sudo", "systemctl", "stop", name)
        administrative("sudo", "rm", "--", destination)
        administrative("sudo", "systemctl", "daemon-reload")
        (Path(tempfile.gettempdir()) / (name + ".lock")).unlink(missing_ok=True)


def rehearsal(image, setup, output, negative_control, host, exercise):
    require(os.environ.get("GITHUB_ACTIONS") == "true", "run this rehearsal on GitHub, not a workstation")
    require(image.startswith("sha256:") and len(image) == 71, "use the immutable local image ID")
    require(host in {"tools", "claude", "codex", "opencode"}, "declare the rehearsed application")
    require(exercise in {"tools", "handoff"} and (exercise != "handoff" or host != "tools"), "declare the exercise and native application")
    require(not negative_control or host == "tools", "detached-worker control has no native application")
    output.mkdir(parents=True, exist_ok=False)
    budget = {**limits(), "memory_bytes": 512 * 1024**2,
              "cpu_seconds": 0.5 if negative_control else 60}
    name = "fr-comparison-" + uuid.uuid4().hex
    started = time.monotonic()
    report = {"setup": setup, "host": host, "exercise": exercise, "negative_control": negative_control, "image": image,
              "model_calls": 0, "passed": False,
              "scope": "All processes in one attempt container, including the scripted host and detached daemon. "
                       "Excludes image download/build, outer controller, Docker service and model inference. "
                       "This is an integration control, not a cost comparison or native-agent trial."}
    def retain_command(command, data, directory, timeout, cap):
        result = invoke(command, data, directory, timeout, cap)
        process, stdout, stderr = result
        report.setdefault("commands", []).append({"argv": command, "process": process,
            "stdout": stdout.decode(errors="replace"), "stderr": stderr.decode(errors="replace")})
        return result

    with isolated_budget(output, budget) as slice_id:
        scope = ContainerResources(budget, slice_id, output, execute=retain_command).start()

        def docker(*arguments, timeout):
            process, stdout, stderr = scope.invoke(DOCKER + list(arguments), b"", output, timeout, 2 * 1024**2)
            require(process["exit_code"] == 0 and not process["stop_reason"],
                    f"Docker command failed: {arguments!r}: {stderr.decode(errors='replace')}")
            return stdout

        try:
            home = "/tmp/home" if host == "tools" else "/home/control"
            docker("create", "--name", name, "--pull=never", "--network=none", "--read-only",
                   "--cap-drop=ALL", "--security-opt=no-new-privileges", "--user=65534:65534",
                   "--cpus=0.5", "--memory=512m", "--memory-swap=512m", "--pids-limit=64",
                   "--log-driver=none", "--workdir=/workspace", "--env=HOME=" + home,
                   "--tmpfs=/workspace:rw,exec,nosuid,nodev,size=256m,mode=1777",
                   "--tmpfs=/tmp:rw,exec,nosuid,nodev,size=64m,mode=1777",
                   *(["--tmpfs=/home/control:rw,exec,nosuid,nodev,size=64m,mode=1777"] if host != "tools" else []),
                   image, "python3", "-c", "import time; time.sleep(150)", timeout=10)
            docker("start", name, timeout=10)
            docker("exec", name, "mkdir", "-p", home, timeout=5)
            if negative_control:
                # The launcher exits. Its child starts a new session and burns CPU.
                # A parent/group sampler would miss it; the kernel slice must stop it.
                script = ("import subprocess,sys,os,json; worker=subprocess.Popen([sys.executable,'-c',"
                          "'while True: pass'],start_new_session=True,"
                          "stdin=subprocess.DEVNULL,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL);"
                          "print(json.dumps({'pid':worker.pid,'worker_pgid':os.getpgid(worker.pid),"
                          "'launcher_pgid':os.getpgrp()}))")
                worker = json.loads(docker("exec", name, "python3", "-c", script, timeout=5))
                report["detached_worker"] = worker
                require(worker["worker_pgid"] != worker["launcher_pgid"], "failure control did not detach its worker")
                deadline = time.monotonic() + 8
                while scope.reason is None and time.monotonic() < deadline:
                    time.sleep(0.05)
                require(scope.reason == "cpu_seconds", "detached CPU worker escaped the shared budget")
            else:
                probe = ["python3", "/probe/comparison_probe.py", "--setup", setup] if host == "tools" else [
                    "python3", "/probe/comparison_handoff_native.py" if exercise == "handoff" else "/probe/comparison_native.py",
                    "--host", host, "--setup", setup]
                data = docker("exec", name, *probe, timeout=100)
                report["probe"] = json.loads(data)
                scope.check()
                report["after_host_exit"] = dict(scope.last)
                if setup == "codegraph" and host == "tools":
                    require(report["probe"]["daemon_alive_after_proxy_exit"], "missing daemon lifecycle control")
                    require(scope.last["populated"] == 1, "daemon left its container budget")
        except Exception as error:
            report["error"] = f"{type(error).__name__}: {error}"
        finally:
            # Capture diagnostics on failure too; a killed container may reject exec.
            # This does not restart it or bypass a stopped kernel budget.
            diagnostic, stdout, stderr = invoke(DOCKER + ["exec", name, "python3", "-c",
                "from pathlib import Path; import json; print(json.dumps({str(p):p.read_bytes()[:16384].decode(errors='replace') "
                "for root in (Path('/workspace'),Path('/tmp/control')) for p in root.rglob('*') if p.is_file() and "
                "p.name in ('mcp.json','mcp.stderr','daemon.log','native.stdout','native.stderr','provider.json','provider-summary.json','native-config.json','handoff-summary.json')}))"],
                b"", output, 5, 256 * 1024)
            report["diagnostics"] = {"process": diagnostic, "stdout": stdout.decode(errors="replace"),
                                     "stderr": stderr.decode(errors="replace")}
            # rm --force kills PID 1 AND detached descendants before the final sample.
            process, stdout, stderr = invoke(DOCKER + ["rm", "--force", name], b"", output, 10, 65536)
            report["cleanup"] = {"process": process, "stdout": stdout.decode(errors="replace"),
                                 "stderr": stderr.decode(errors="replace")}
            retained = scope.finish()
            report["resources"] = retained
            report["elapsed_seconds"] = time.monotonic() - started
        try:
            require("error" not in report, report.get("error", ""))
            checked = audit(retained, budget)
            require(process["exit_code"] == 0 and not process["stop_reason"], "container removal failed")
            require(checked["complete"], "container processes survived cleanup")
            require(checked["stop_reason"] == ("cpu_seconds" if negative_control else None), "unexpected resource stop")
            require(checked["cpu_seconds"] > 0 and checked["memory_peak_bytes"] > 0, "missing whole-container counters")
            report["passed"] = True
        except Exception as error:
            report["error"] = f"{type(error).__name__}: {error}"
            raise
        finally:
            (output / "result.json").write_text(json.dumps(report, indent=2, allow_nan=False) + "\n")
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--image", required=True)
    parser.add_argument("--setup", choices=("files", "fr", "codegraph"), required=True)
    parser.add_argument("--host", choices=("tools", "claude", "codex", "opencode"), required=True)
    parser.add_argument("--exercise", choices=("tools", "handoff"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--negative-control", action="store_true")
    args = parser.parse_args()
    rehearsal(args.image, args.setup, args.output.resolve(), args.negative_control, args.host, args.exercise)
