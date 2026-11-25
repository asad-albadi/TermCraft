# Independent fixture recovery review

**Accepted for parent integration. No important findings.** Review is limited to the single-file fixture repair; parent integration and the fresh required full gate remain pending.

Verified MAIN and archived base SHA256 are both `3026341c6f1cbee61b0135953d425b54523a40f43730c16b568dd40ee5374faf`. Archived proposed and restored private source are both `308099082909608838fd9d2a7fe4ac8bdd1eb9a7cdacc337b57e7dd373876a95`. Independently regenerated the normalized unified diff and confirmed exact equality with `candidate.patch`. The sole changed line is the embedded C fixture in the Linux test module. Production source before and after that module is byte-identical.

The bounded `fgets` consumes the complete existing three-byte request before emitting the reply and returning. This removes the early-exit/write race without swallowing EPIPE, adding a sleep, or changing production process, permission, sealing, or grant handling. The existing polling sleep elsewhere in the Rust test is unchanged.

The sealed-image mutation must still fail with EPERM; installed bytes are still replaced with the false fixture; the spawn still receives the verified descriptor; and the output must still contain original `ok:true`. The archived counterexample differs from the proposed source only by passing the reopened replacement descriptor into the spawn. Its recorded failure is at the retained original-true assertion, after the EPERM assertion, demonstrating that the repaired protocol does not mask execution of replacement bytes. The counterexample has been restored out of the private candidate.

Independently reran the exact-template probe: base exited before input and reproduced BrokenPipe (expected status 1); proposed accepted the request and returned the expected original response (status 0). Inspected the 40 recorded successful exact-filter repeats, focused build/restoration logs, and the four-test plugin-filter pass. Independently executed the existing focused binary once: 1 passed, 0 failed, 258 filtered. Binary SHA256 at review: `d378862ff094f21aa9eca70ec5e4a5e86e278c5dd83461e01c1cbba97cae4520`. No Cargo rebuild or broad suite was run by this reviewer.

All reviewer-owned subprocesses completed and were reaped. No MAIN implementation files were modified by this review; only this requested review artifact was written. Approval applies to the exact proposed hash above; the parent must recheck the MAIN base before applying and run the required fresh gate after integration.
