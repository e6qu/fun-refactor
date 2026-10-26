"""Check process-group accounting without launching a system-wide ps command."""
import ctypes
import errno
import os
import signal
import subprocess
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

from . import bounded_host as host


class DarwinSampling(unittest.TestCase):
    def setUp(self):
        self.library = SimpleNamespace(proc_listpids=Mock(side_effect=self.list_pids),
                                       proc_pidinfo=Mock(side_effect=self.pid_info))
        self.system = SimpleNamespace(mach_timebase_info=Mock(side_effect=self.timebase))
        self.loader = patch.object(ctypes, 'CDLL', side_effect=lambda name, **_: {
            '/usr/lib/libproc.dylib': self.library,
            '/usr/lib/libSystem.B.dylib': self.system}[name])
        self.loader.start()
        self.addCleanup(self.loader.stop)

    def timebase(self, pointer):
        value = ctypes.cast(pointer, ctypes.POINTER(host.DarwinTimebase)).contents
        value.numer, value.denom = 3, 2
        return 0

    def list_pids(self, kind, group, buffer, size):
        self.assertEqual((kind, group), (2, 123))
        buffer[0], buffer[1] = 201, 202
        return 2 * ctypes.sizeof(ctypes.c_int)

    def pid_info(self, pid, kind, argument, pointer, size):
        self.assertEqual((kind, argument, size), (4, 0, 96))
        value = ctypes.cast(pointer, ctypes.POINTER(host.DarwinTaskInfo)).contents
        value.resident_size = 1024 * (pid - 200)
        value.total_user, value.total_system = 10_000_000, 20_000_000
        return size

    def test_aggregate_bytes_and_mach_ticks_without_subprocesses(self):
        with patch.object(host.subprocess, 'run', side_effect=AssertionError('must not launch ps')):
            self.assertEqual(host.darwin_sample(123), (3072, {201: 0.045, 202: 0.045}))

    def test_empty_group_and_truncated_inventory(self):
        self.library.proc_listpids.side_effect = None
        self.library.proc_listpids.return_value = 0
        self.assertEqual(host.darwin_sample(123), (0, {}))
        for size in (4096 * ctypes.sizeof(ctypes.c_int), 3):
            self.library.proc_listpids.return_value = size
            with self.subTest(size=size), self.assertRaisesRegex(ValueError, 'truncated or malformed'):
                host.darwin_sample(123)

    def test_only_exited_processes_can_disappear_without_failing(self):
        def failure(*arguments):
            ctypes.set_errno(errno.ESRCH)
            return 0
        self.library.proc_pidinfo.side_effect = failure
        self.assertEqual(host.darwin_sample(123), (0, {}))
        def denied(*arguments):
            ctypes.set_errno(errno.EACCES)
            return 0
        self.library.proc_pidinfo.side_effect = denied
        with self.assertRaises(PermissionError):
            host.darwin_sample(123)

    def test_unreadable_inventory_timebase_and_short_task_info_fail_closed(self):
        def denied(*arguments):
            ctypes.set_errno(errno.EPERM)
            return 0
        self.library.proc_listpids.side_effect = denied
        with self.assertRaises(PermissionError):
            host.darwin_sample(123)
        self.library.proc_listpids.side_effect = self.list_pids
        self.library.proc_pidinfo.side_effect = None
        self.library.proc_pidinfo.return_value = 8
        with self.assertRaises(OSError):
            host.darwin_sample(123)
        self.system.mach_timebase_info.side_effect = None
        self.system.mach_timebase_info.return_value = 1
        with self.assertRaisesRegex(ValueError, 'timebase'):
            host.darwin_sample(123)


@unittest.skipUnless(sys.platform == 'darwin', 'requires the macOS process API')
class NativeDarwinSampling(unittest.TestCase):
    def test_stopped_parent_and_worker_remain_counted(self):
        worker = 'import os,time; x=bytearray(4*1024*1024); print(os.getpid(),flush=True); time.sleep(10)'
        command = ('import os,subprocess,sys,time; x=bytearray(4*1024*1024); '
                   f'subprocess.Popen([sys.executable,"-c",{worker!r}]); '
                   'print(os.getpid(),flush=True); time.sleep(10)')
        process = subprocess.Popen([sys.executable, '-c', command], stdout=subprocess.PIPE,
                                   text=True, start_new_session=True)
        try:
            members = {int(process.stdout.readline()), int(process.stdout.readline())}
            os.killpg(process.pid, signal.SIGSTOP)
            with patch.object(host.subprocess, 'run', side_effect=AssertionError('must not launch ps')):
                memory, cpu = host.sample(process.pid)
            self.assertEqual(set(cpu), members)
            self.assertGreater(memory, 8 * 1024**2)
            self.assertTrue(all(value > 0 for value in cpu.values()))
        finally:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=2)
            process.stdout.close()


if __name__ == '__main__':
    unittest.main()
