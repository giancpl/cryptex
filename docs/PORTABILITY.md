# Standard LaTeX and Overleaf portability

J4 treats portability as a checked property of representative ordinary project
directories. The selected matrix covers pdfLaTeX, XeLaTeX, LuaLaTeX, multipass
references, BibTeX, Biber, TikZ, local styles, subfiles, magic roots, and the real
`cryptocode` package.

Run the structural gate with:

```bash
pnpm portability:validate
```

The gate performs the following operations:

1. Requires the exact representative set to be declared `portable` in the
   acceptance manifest.
2. Copies each project to a fresh directory outside its checked-in CrypTex fixture
   location.
3. Rejects symlinks, special files, `.cryptex*` metadata, and proprietary
   `\cryptex...` LaTeX commands.
4. Confirms every copied file has the same SHA-256 content and that the declared
   root remains present.
5. Leaves no CrypTex runtime or database beside the project.

Compilation is performed by the J1 command:

```bash
pnpm acceptance:compile -- /absolute/path/to/ordinary/tex/bin/platform
```

It copies projects again and invokes the supplied ordinary `latexmk` executable
with normal engine flags and `-norc`; no CrypTex executable, macro, metadata, or
runtime participates. The path is mandatory, so validation cannot silently use a
developer-global TeX installation.

Artifact comparison is semantic rather than byte-for-byte. Every successful fixture
must produce the correctly stemmed PDF and compressed SyncTeX file. PDFs must contain
a PDF header and EOF trailer; SyncTeX must have the gzip signature. Complete PDF bytes
are not compared because TeX engines can embed timestamps, identifiers, font subsets,
and distribution-specific metadata without changing document compatibility.

The structural gate runs in ordinary CI. Full compilation remains part of the managed
toolchain and clean-machine J5/J6 gates because CI intentionally does not download a
second large TeX distribution merely to duplicate the pinned offline payload.
