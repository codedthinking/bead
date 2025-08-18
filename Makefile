.PHONY: test clean executables shiv

test:
	tox

executables:
	dev/build.py

shiv:
	shiv -o executables/bead.shiv -c bead -p '/usr/bin/python -sE' .
