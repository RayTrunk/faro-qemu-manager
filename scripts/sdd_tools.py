import sys
import os
import re
import subprocess

def extract_brief(plan_path, task_num, out_path):
    with open(plan_path, 'r', encoding='utf-8') as f:
        content = f.read()

    pattern = rf"(###\s+Task\s+{task_num}:.*?)(?=\n###\s+Task\s+\d+:|\Z)"
    match = re.search(pattern, content, re.DOTALL)
    if not match:
        print(f"Error: Task {task_num} not found in {plan_path}", file=sys.stderr)
        sys.exit(1)

    os.makedirs(os.path.dirname(os.path.abspath(out_path)), exist_ok=True)
    with open(out_path, 'w', encoding='utf-8') as f:
        f.write(match.group(1).strip() + "\n")
    print(f"Wrote brief for Task {task_num} to {out_path}")

def review_package(base, head, out_path):
    os.makedirs(os.path.dirname(os.path.abspath(out_path)), exist_ok=True)
    log_out = subprocess.check_output(["git", "log", "--oneline", f"{base}..{head}"], text=True, encoding="utf-8", errors="replace")
    stat_out = subprocess.check_output(["git", "diff", "--stat", f"{base}..{head}"], text=True, encoding="utf-8", errors="replace")
    diff_out = subprocess.check_output(["git", "diff", "-U10", f"{base}..{head}"], text=True, encoding="utf-8", errors="replace")

    with open(out_path, 'w', encoding='utf-8') as f:
        f.write(f"# Review package: {base}..{head}\n\n")
        f.write("## Commits\n" + log_out + "\n\n")
        f.write("## Files changed\n" + stat_out + "\n\n")
        f.write("## Diff\n" + diff_out + "\n")
    print(f"Wrote review package to {out_path}")

if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "brief":
        extract_brief(sys.argv[2], int(sys.argv[3]), sys.argv[4])
    elif cmd == "review":
        review_package(sys.argv[2], sys.argv[3], sys.argv[4])
    else:
        print(f"Unknown command: {cmd}", file=sys.stderr)
        sys.exit(1)
