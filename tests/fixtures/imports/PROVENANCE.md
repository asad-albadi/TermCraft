# Import fixture provenance

These are authored synthetic fixtures, not captured native exports. Moba fixture uses SSH field indexes and folder structure from [Ruzgfpegk’s firsthand study of version 26.3](https://gist.github.com/Ruzgfpegk/ab597838e4abbe8de30d7224afd062ea); optional later fields are deliberately absent to exercise older/truncated records without inventing values. Exact 26.6 export compatibility remains pending.

PuTTY fixture uses setting names from [official PuTTY 0.85 sources](https://the.earth.li/~sgtatham/putty/0.85/putty-0.85.tar.gz), archive SHA256 13fd4db2936d03b73812a7bcc2a658e4dd29cc776a56c3670a7fc6f1a0ee8af8. The .reg syntax is authored; Windows Registry export round-trip remains pending. Hostnames are reserved .test examples and credential strings are non-secret test markers. No fixture connects or imports registry state.

[Official Mobatek CSV documentation](https://blog.mobatek.net/post/mobaxterm-import-csv/) provides an independent documented input format. Encoding variants are generated in the tests from these owned contents.
