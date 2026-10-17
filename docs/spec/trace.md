# Traceability — PROTECTED file

Maps every claim id to the tests that prove it and to the oracle class of each
test. `tools/check_trace.sh` fails when

- a test names no claim id,
- a `CONFIRMED AGAINST ORIGINAL BINARY` claim has no test in oracle class 1, 2 or 3, or
- the number of tests for a claim has dropped.

## Oracle classes

| Class | Source |
| --- | --- |
| 1 | Bytes of the original executable, read by a separate extraction tool |
| 2 | Closure on real data: exact decoded size, exact consumption, zero trailing bytes |
| 3 | Known plaintext |
| 4 | An independent reference implementation written by the test author |
| 5 | Hand computed vectors in the implementation guide |
| 6 | Algebraic laws |

Not an oracle: the output of the code under test, a snapshot of that output,
Community Edition output, or a value copied from a failure message.

## Claim to test map

Populated by the test-author role in wave 2. Empty at scaffold time.