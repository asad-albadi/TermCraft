# Independent emulated-runtime evidence review

Verdict: **accepted as qualified emulated-original observations and bounded investigation records**. This is neither native Windows qualification nor successful Games/settings exposure. Reviewed 2026-11-25 without any new original/game/GUI launch. No product code or completion counts changed.

## Checks actually performed

Recomputed all 42 retained manifest SHA-256s and byte lengths and all eight listed private-download hashes/sizes: no mismatch. Inspected signature status logs, retained package/license metadata, containment scripts, read-only Win32 helper source and output, startup screenshot, partial INI qualifications, and the retained loader-section timeout log. The screenshot visibly shows a mapped main interface, Games toolbar/menu labels, theme chooser and UNREGISTERED VERSION footer; it does not show an expanded Games catalog. Exact executable identity comes from the hash-linked supplied input, not from interpreting the footer as an edition certificate.

The three signature logs contain GOODSIG and VALIDSIG for the recorded Peter Jung/Andreas Radke fingerprints. They also explicitly contain TRUST_UNDEFINED and an uncertified-key warning. Thus the evidence supports cryptographic signature verification against the recorded locally imported key material and the reported official packager correspondence, not an independently completed Web-of-Trust certification. Download SHA identities remain reproducible. Retained Wine LGPL and Xvfb/twm license materials are suitable research provenance; this review grants no product-redistribution conclusion. The locally built Win32 helper uses enumeration/query calls and contains no input-send or dialog-accept action.

The run script provides a bounded private namespace with read-only host executable/library resources, private home/prefix/work, private display and network namespace, and an explicit timeout. It does not expose the host desktop or user home. The namespace is still an execution environment using host libraries, so it is not a native Windows fixture. Retained wine.log explicitly records loader_section critical-section timeouts; the helper log reports the main window and settings/session classes as invisible and contains no menu rows. Neither absence demonstrates that original menus/features are absent.

## Accepted scope and unresolved claims

Accepted measured scope is the mapped startup UI and partial generated startup INI under the recorded Wine setup. ZIP entry catalogs are recovered companion/vendor fixture evidence; current Sqlite3.mxt3 and historical apt-cyg sources are identified supporting research, not proven universal 26.6 packaging/dependency semantics. The attempted stable fallback is a timed-out request, not a tested alternative runtime.

No observed expanded Games popup, full settings/default set, export format, effective Personal limits, install resolver, screensaver timing or gameplay is established. File names from failed games/settings input attempts must not be promoted to measured results. The static-recovery review may independently justify declared targets; it cannot retroactively turn these runtime failures into exposure evidence.

No blocking factual overclaim was found in the qualified handoff. The full cleanup statement is the investigator's historical audit record, not a newly repeated global process audit in this review. This reviewer created no persistent jobs; the bounded decoder replay for the separate static review exited and its temporary output was removed. Do not kill unrelated processes or advance strict/native/full counts from either handoff.
