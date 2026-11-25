# D02 incremental review — 2026-11-24

Read-only independent review of this increment, under the Superpowers requesting-code-review workflow; no Git range exists. Reviewed typed RDP credentials, vault UI integration and automatic fullscreen. Unfinished device/admin providers were excluded from the narrow review, not from scope.

No Critical findings. One Important finding: the automatic fullscreen path could receive a detached-window ID while egui's `EmbeddedWindow` fallback retained the root viewport context. It would store fullscreen/restore geometry against the detached window and send the native command to root. The manual path already guarded embedded windows.

Reproduction: the new `embedded_detached_view_does_not_fullscreen_the_root_or_consume_its_request` uses real egui `show_viewport_immediate` with `set_embed_viewports(true)`. Before the fix it failed with “Request must survive until a native detached viewport exists” (2 pass / 1 fail): [RED](fullscreen-review-red.log). An earlier harness compile error from using Debug on ViewportClass was corrected before recording behavioral RED.

Fix: viewport IDs are centralized in the window model. The automatic fullscreen decision accepts the actual rendered viewport ID and refuses to consume the request or mutate either window when that viewport does not match the target. The production paint path passes `ctx.viewport_id()`. Root/native detached behavior and geometry restoration remain covered.

[GREEN](fullscreen-review-green.log): three fullscreen tests and the encrypted-vault binding test pass (4 total). The reviewer found no additional Important credential defect; actual real-RDP/UI coverage remains blocked and is not inferred from review or local tests. Full-suite, app-build and rebuild-check results after this fix are recorded separately.
