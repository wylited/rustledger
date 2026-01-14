.PHONY: srpm

SPECFILE := ../packaging/rpm/rustledger.spec

srpm:
	dnf install -y rpmdevtools rpm-build
	spectool -g -C $(outdir) $(SPECFILE)
	rpmbuild -bs $(SPECFILE) \
		--define "_sourcedir $(outdir)" \
		--define "_srcrpmdir $(outdir)"
