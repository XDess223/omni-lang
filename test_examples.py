# test_examples.py
# Automated integration test runner for all Omni language examples.

import os
import subprocess
import sys

# Directory containing the examples
EXAMPLES_DIR = "examples"

# Configuration for individual files:
# - Expected exit code (0 for success, 1 for compilation/runtime failure)
# - Whether to skip the file entirely (e.g. libraries without a main entrypoint)
CONFIGS = {
    "stdlib.omni": {"skip": True},
    "omni_whitepaper_showcase.omni": {"skip": True},  # Theoretical mockup syntax
    "in_mode.omni": {"expected_exit_code": 1},  # Expected to fail semantic checks
    "null_safety.omni": {"expected_exit_code": 1},  # Expected to fail semantic checks
}

def get_examples():
    if not os.path.exists(EXAMPLES_DIR):
        print(f"Error: Directory '{EXAMPLES_DIR}' not found.")
        sys.exit(1)
    
    files = [f for f in os.listdir(EXAMPLES_DIR) if f.endswith(".omni")]
    return sorted(files)

def run_test(filename):
    filepath = os.path.join(EXAMPLES_DIR, filename)
    config = CONFIGS.get(filename, {"expected_exit_code": 0, "skip": False})
    
    if config.get("skip", False):
        return "SKIP", "Library file (no main entrypoint)"

    cmd = ["cargo", "run", "--bin", "omni", "--quiet", "--", "run", filepath]
    
    try:
        # Run command, capturing stdout and stderr
        result = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        
        expected = config["expected_exit_code"]
        actual = result.returncode
        
        if actual == expected:
            if expected != 0:
                return "PASS", f"Failed as expected with code {actual}"
            return "PASS", "Executed successfully"
        else:
            return "FAIL", f"Expected exit code {expected}, got {actual}\nError output:\n{result.stderr or result.stdout}"
            
    except Exception as e:
        return "FAIL", f"Execution error: {str(e)}"

def main():
    print("==================================================")
    print("      Omni Integration Examples Test Runner       ")
    print("==================================================")
    
    examples = get_examples()
    passed = 0
    failed = 0
    skipped = 0
    
    results = []
    
    for example in examples:
        print(f"Testing {example:.<35}", end="", flush=True)
        status, reason = run_test(example)
        
        if status == "PASS":
            print("[\033[92mPASS\033[0m]")
            passed += 1
        elif status == "SKIP":
            print("[\033[94mSKIP\033[0m]")
            skipped += 1
        else:
            print("[\033[91mFAIL\033[0m]")
            print(f"  -> {reason}")
            failed += 1
            
        results.append((example, status, reason))
        
    print("==================================================")
    print("                   Summary                        ")
    print("==================================================")
    print(f"  Total:    {len(examples)}")
    print(f"  Passed:   {passed}")
    print(f"  Failed:   {failed}")
    print(f"  Skipped:  {skipped}")
    print("==================================================")
    
    if failed > 0:
        sys.exit(1)
    sys.exit(0)

if __name__ == "__main__":
    main()
