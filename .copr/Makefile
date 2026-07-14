.PHONY: srpm

srpm:
	@echo "Creating source tarball..."
	cargo vendor > /dev/null 2>&1 || true
	mkdir -p .cargo
	echo '[source.crates-io]' > .cargo/config.toml
	echo 'replace-with = "vendored-sources"' >> .cargo/config.toml
	echo '' >> .cargo/config.toml
	echo '[source.vendored-sources]' >> .cargo/config.toml
	echo 'directory = "vendor"' >> .cargo/config.toml
	tar --transform 's,^\./,libinput-rs-0.1.1/,' \
	    --exclude='./.git' \
	    --exclude='./target' \
	    --exclude='./build_workspace' \
	    -czf $(outdir)/libinput-rs-0.1.1.tar.gz \
	    ./vendor ./.cargo ./src ./Cargo.toml ./Cargo.lock ./systemd ./libinput-elan-reset.service ./libinput-rs.spec ./README.md ./build_package.sh
	@echo "Building SRPM..."
	rpmbuild -bs $(spec) \
	    --define "_sourcedir $(outdir)" \
	    --define "_specdir $(outdir)" \
	    --define "_builddir $(outdir)" \
	    --define "_srcrpmdir $(outdir)" \
	    --define "_rpmdir $(outdir)"
