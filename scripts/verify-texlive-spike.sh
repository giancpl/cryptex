#!/bin/sh
set -eu

if [ "$#" -ne 1 ] || [ ! -d "$1" ]; then
  echo "usage: $0 /absolute/path/to/texlive/bin/PLATFORM" >&2
  exit 2
fi

tex_bin=$1
case "$tex_bin" in
  /*) ;;
  *) echo "TeX binary directory must be absolute" >&2; exit 2 ;;
esac

for command in latexmk pdflatex xelatex lualatex bibtex biber kpsewhich synctex; do
  if [ ! -x "$tex_bin/$command" ]; then
    echo "missing managed executable: $command" >&2
    exit 1
  fi
done

fixture_dir=$(CDPATH= cd -- "$(dirname -- "$0")/../tests/fixtures/toolchain-spike" && pwd)
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/cryptex-texlive-spike.XXXXXX")
trap 'rm -rf -- "$work_dir"' EXIT HUP INT TERM
cp -R -- "$fixture_dir/." "$work_dir/"

export PATH="$tex_bin:/usr/bin:/bin"
export SOURCE_DATE_EPOCH=0
cd -- "$work_dir"

latexmk -norc -pdf -interaction=nonstopmode -halt-on-error -file-line-error -synctex=1 minimal.tex
latexmk -norc -pdf -interaction=nonstopmode -halt-on-error -file-line-error -synctex=1 cryptocode.tex
latexmk -norc -pdf -interaction=nonstopmode -halt-on-error -file-line-error -synctex=1 tikz.tex
latexmk -norc -pdf -interaction=nonstopmode -halt-on-error -file-line-error -synctex=1 biber.tex
latexmk -norc -xelatex -interaction=nonstopmode -halt-on-error -file-line-error -synctex=1 unicode-engines.tex
latexmk -norc -lualatex -interaction=nonstopmode -halt-on-error -file-line-error -synctex=1 unicode-engines.tex

for artifact in minimal.pdf cryptocode.pdf tikz.pdf biber.pdf unicode-engines.pdf minimal.synctex.gz; do
  test -s "$artifact"
done

echo "TeX Live spike fixtures passed"
