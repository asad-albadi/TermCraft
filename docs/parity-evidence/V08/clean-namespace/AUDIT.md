# Final export audit and sealed evidence

The post-run rootfs audit independently compared **27304 regular file contents** against the owned exported archive, verified **3961 link entries** resolve within the scratch root, found zero archive entries beneath link ancestors and zero escaping resolved filesystem links. Only the two empty mountpoint directories were added to the extraction. This supplements the extraction's lexical traversal/link checks with actual resolved-link containment and full exported file-content equivalence. `audit-rootfs.py` and `rootfs-audit.json` retain the check and result.

`SHA256SUMS.json` is the untouched first 60-file seal. `SHA256SUMS-final.json` adds the audit artifacts and this note while also sealing the first index; it does not rewrite an earlier seal. All first-index entries were freshly checked before writing the final index.
