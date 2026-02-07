# Test file for subprocess module

import subprocess

# ===========================================================
# Test subprocess.run basic returncode behavior
# ===========================================================

result1 = subprocess.run(["echo", "hello"])
print("result1.returncode:", result1.returncode)
assert result1.returncode == 0, "echo should return 0"

result_false = subprocess.run(["false"])
print("result_false.returncode:", result_false.returncode)
assert result_false.returncode != 0, "false should return non-zero"
print("subprocess.run basic returncode tests passed!")

# ===========================================================
# Test capture_output and CompletedProcess fields
# ===========================================================

result2 = subprocess.run(["echo", "test output"], True, False)
assert result2.returncode == 0, "echo should return 0"

stdout2: str | None = result2.stdout
stderr2: str | None = result2.stderr
assert stdout2 is not None, "stdout should not be None when capture_output=True"
assert stderr2 is not None, "stderr should not be None when capture_output=True"
if stdout2 is not None:
    assert "test output" in stdout2, "stdout should contain test output"
assert stderr2 == "", "stderr should be empty for echo"

args2: list[str] = result2.args
assert len(args2) == 2, "echo invocation should include exactly two args"
assert args2[0] == "echo", "first command element should be echo"
assert args2[1] == "test output", "second command element should be payload"
print("capture_output and fields tests passed!")

# ===========================================================
# Test no-capture mode
# ===========================================================

result3 = subprocess.run(["echo", "not captured"], False, False)
assert result3.returncode == 0, "echo should return 0"
assert result3.stdout is None, "stdout should be None when capture_output=False"
assert result3.stderr is None, "stderr should be None when capture_output=False"
print("no-capture tests passed!")

# ===========================================================
# Test stderr capture and keyword arguments
# ===========================================================

result4 = subprocess.run(["sh", "-c", "echo out && echo err >&2"], capture_output=True, check=False)
assert result4.returncode == 0, "shell command should succeed"
stdout4: str | None = result4.stdout
stderr4: str | None = result4.stderr
assert stdout4 is not None, "stdout should be captured"
assert stderr4 is not None, "stderr should be captured"
if stdout4 is not None:
    assert "out" in stdout4, "stdout should contain out"
if stderr4 is not None:
    assert "err" in stderr4, "stderr should contain err"
print("stderr capture tests passed!")

# ===========================================================
# Test check=True success path
# ===========================================================

result5 = subprocess.run(["echo", "check ok"], capture_output=True, check=True)
assert result5.returncode == 0, "check=True should pass for successful command"
stdout5: str | None = result5.stdout
assert stdout5 is not None, "stdout should be captured when capture_output=True"
if stdout5 is not None:
    assert "check ok" in stdout5, "stdout should contain check payload"
print("check=True success test passed!")

# ===========================================================
# Test from-import form
# ===========================================================

from subprocess import run

result6 = run(["echo", "from import"], capture_output=True, check=False)
assert result6.returncode == 0, "from-import run should execute command"
stdout6: str | None = result6.stdout
assert stdout6 is not None, "stdout should be captured when capture_output=True"
if stdout6 is not None:
    assert "from import" in stdout6, "captured stdout should contain payload"
print("from-import run test passed!")

print("All subprocess module tests passed!")
