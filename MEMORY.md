## GOTCHA

- Symptom: temporary Git fixture commits may invoke a developer's signing helper and fail. Cause: Git inherits global `commit.gpgsign`. Fix: set repository-local `commit.gpgsign=false` in every committing fixture.

## TASTE

- Prefer aru CLI structure and terminology to follow uv and Cargo where domain semantics align; retain aru's fail-closed behavior over superficial parity.

## CONVENTIONS
