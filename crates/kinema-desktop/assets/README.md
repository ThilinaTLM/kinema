# Bundled desktop assets

Pinned sources (downloaded 2026-10-18):

- **Inter 4.1**: https://github.com/rsms/inter/releases/download/v4.1/Inter-4.1.zip
  Only `extras/ttf/Inter-{Regular,Medium,SemiBold,Bold}.ttf` are retained, unchanged.
  Release static faces identify internally as 4.001. `LICENSE.txt` is retained as
  `fonts/OFL-Inter.txt` (SIL OFL 1.1).
- **JetBrains Mono 2.304**: https://github.com/JetBrains/JetBrainsMono/releases/download/v2.304/JetBrainsMono-2.304.zip
  Only `fonts/ttf/JetBrainsMono-Regular.ttf` is retained, unchanged; `OFL.txt` is
  retained as `fonts/OFL-JetBrainsMono.txt` (SIL OFL 1.1).
- **Lucide 0.468.0**: https://github.com/lucide-icons/lucide/tree/0.468.0/icons
  Each SVG comes from `https://raw.githubusercontent.com/lucide-icons/lucide/0.468.0/icons/<filename>`.
  SVG strokes use `stroke-width="1.5"` instead of upstream `2` for desktop-v3 §2.4.
  `star-filled.svg` is derived from the same pinned `star.svg`, with
  `fill="currentColor"` instead of `none`; its stroke remains `currentColor`.
  `circle.svg` and `circle-dot.svg` come from the same pinned release.
  License: https://raw.githubusercontent.com/lucide-icons/lucide/0.468.0/LICENSE
  retained as `icons/LICENSE-lucide.txt` (ISC; includes Feather attribution).
- **TMDB logo**: existing, unchanged; source URL and terms are in root `NOTICE`.

Verification: `file` identified all five fonts as TrueType and all 29 icons as SVG;
SVGs were also parsed as XML. SHA-256 below covers the shipped bytes (including
the documented Lucide stroke and fill changes), not a claim of upstream signed provenance.

## SHA-256

```text
288316099b1e0a47a4716d159098005eef7c0066921f34e3200393dbdb01947f  fonts/Inter-Bold.ttf
97ad806f526e41546d46365bb3a393145f75b7b1568913db74549ad8b8dba872  fonts/Inter-Medium.ttf
40d692fce188e4471e2b3cba937be967878f631ad3ebbbdcd587687c7ebe0c82  fonts/Inter-Regular.ttf
78a843fade9d4612a5567302fb595b56976eb5fcebf4fea5a5912d638bafcde3  fonts/Inter-SemiBold.ttf
a0bf60ef0f83c5ed4d7a75d45838548b1f6873372dfac88f71804491898d138f  fonts/JetBrainsMono-Regular.ttf
262481e844521b326f5ecd053e59b98c8b2da78c8ee1bdbb6e8174305e54935a  fonts/OFL-Inter.txt
30f0c136e3c88e422d0791acd97238870f9054a9729bc34cf2ff0d4ed8cac4ad  fonts/OFL-JetBrainsMono.txt
1e7290b35280a048667bbf0ebabac1c7fd52a75300e8b2946ac165715997f2bc  icons/LICENSE-lucide.txt
7d835c105fb5694355c81d420e98de996d93cd8e4b6e779d26411e88fd62f472  icons/arrow-down-up.svg
1a2f1a16f2695e53667f634ab5474aa388eb8d74597305f1a88a87ef7a6b4efd  icons/arrow-left.svg
a2008e4d15d478da745edb2f45a9d4753d5cad824ff82fb563402009ec0f2962  icons/check.svg
2bf4104082f10ba647c8dba1b186a2450171cf9a50f52043e83e8b28a124e998  icons/chevron-down.svg
fc7e8134bb0e3ead3e1bfecb26949615c715fcd993489307dfdfa1719752b2bb  icons/chevron-left.svg
63abe3521141698f0be3b2298a2fc2eea2342d5acd8becf86dcf989a18bdeab9  icons/chevron-right.svg
1d2b0ec45e5523b020e0a83697b4cb78476fc23d9fd79d823662dfeef9d26792  icons/chevron-up.svg
fde5e37d65d5b947446dfcdf1cfa88ec204f442b028482dea7320ed551e809c7  icons/circle-alert.svg
387846214b75fe4b181ef9f29b3206cc277152bf4258af3b382cae7f2ad1031f  icons/circle-dot.svg
e30fd15078567d0126b7797ace92dc921bb78178c4d7ee63e0eeb8ad02062f90  icons/circle.svg
56823bca60645cd0ab89db03ffd4e2410254bb5f947677a080dd7b8118472804  icons/copy.svg
2ab99f6f38c30d02ad69409cce314054ec7d23f85f16bf02b2a1d90ad67aa596  icons/download.svg
edf70837df72da01051463d2ddec41d7be62ac6016ff538f7a5adc786b44f0c1  icons/ellipsis.svg
f94e7a2947755a5a682e14f625c1d5902b0c6434ff170d60fa2688b053b5e6af  icons/external-link.svg
58f5cde8f76ee63759e8a8740aedd9372ff23fd425df4edc3e44cd018f3cc6bc  icons/film.svg
df33e389e37743df2cfa77a17068faf64bd570b0d4068676dc24613b0b4c9a44  icons/folder-open.svg
76a36e2e67dd725ae08fd7d2f423be2c90ef76d0c412595d09f21fed4c3b8f1e  icons/info.svg
cc11af832974e1e32f11112962ba2db2e3e06b34b12f7b3cef16afd9b129b620  icons/key-round.svg
405ae9ec5d0a0ff5ec1a0048f20313c62806a02032112aea4bf2157bbcf8818b  icons/menu.svg
9de83a5fdb2e0f1f3d49fa001d5270fd75cf1fa04dbe7a841dbfcb64c2dc81f6  icons/play.svg
35dc66731835f1ebb661f15f7ff7e0ff87f4b65c2568a1a105ffdf66191a662e  icons/refresh-cw.svg
bb9bed5b40a5d56d8e901e87f193aee5ca42ced053a4a358cbbe7224344997de  icons/search.svg
2618129f293dea36f11056f0af9b889b8b2b5b18bbb6a5a7f48945f0d02ac2a1  icons/settings.svg
2252c4bc0979927d89dfe119b14297c0485e9e6ed5ec082e7df21def26aa3632  icons/square.svg
61e3de84b658283bb3e60ce827108128d044066b4d9ca21aaf5e9dd5faa6faab  icons/star-filled.svg
d0f90e260faf6e882d5f536bda7a18752de048a44c2ce7b7afd60f841f7beb7f  icons/star.svg
a994fad4185ecbf309615023785731f8e23a5bbd863bd565f2d9c788a3c59d49  icons/triangle-alert.svg
f5d672e1d4fb680360dd3e62f7553f59be4df02422326146e43a8fc37ccd5324  icons/tv.svg
ab35d4780f28dc8527a31066e7c19732b4e5ceb7dab6d6bf0e43d1ffc08914e5  icons/x.svg
8e7b30f73a4020692ccca9c88bafe5dcb6f8a62a4c6bc55cd9ba82bb2cd95f6c  tmdb-logo.svg
```

Verify from this directory: `awk 'NF == 2 && length($1) == 64' README.md | sha256sum -c -`.
