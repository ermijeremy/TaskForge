"""Check the fixture inventory's syntax without executing any submitted program."""
import json
from pathlib import Path

root = Path(__file__).resolve().parent.parent
submissions = root / "fixtures" / "submissions"
catalog = json.loads((submissions / "catalog.json").read_text())
checked = 0
intentional_errors = 0
for fixture in catalog["fixtures"]:
    relative = Path(fixture["path"])
    if relative.is_absolute() or len(relative.parts) != 1 or relative.suffix != ".py":
        raise ValueError(f"invalid source path: {relative}")
    source = submissions / relative
    expected_error = fixture["category"] == "syntax_error"
    try:
        compile(source.read_bytes(), str(source), "exec")
    except SyntaxError:
        if not expected_error:
            raise
        intentional_errors += 1
    else:
        if expected_error:
            raise AssertionError(f"expected an intentional syntax error in {source}")
    checked += 1
if checked != 16 or intentional_errors != 1:
    raise AssertionError("expected 16 named fixtures and exactly one intentional syntax error")
print(f"Checked {checked} Python fixtures: 15 parsed; 1 intentional syntax error. No source executed.")
