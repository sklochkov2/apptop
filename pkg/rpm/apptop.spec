# RPM spec for building apptop from source.
#
# To build from a source tarball:
#   rpmbuild -ba apptop.spec \
#     --define "_sourcedir /path/to/tarball/dir"
#
# The release pipeline produces pre-built static (musl) RPMs instead;
# this spec is provided for distro packagers who prefer source builds.

Name:           apptop
Version:        @VERSION@
Release:        1%{?dist}
Summary:        Top-like memory usage viewer aggregated by application

License:        Unlicense
URL:            https://github.com/sklochkov2/apptop
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo >= 1.85
BuildRequires:  gcc

%description
A top-like terminal utility that aggregates memory usage by application
rather than by individual process. It reads /proc/<pid>/smaps_rollup and
aggregates proportional set size (PSS) and swap usage per application,
using cgroup scope names, environment variable hints, and
interpreter-aware cmdline parsing to identify applications.

%prep
%autosetup

%build
cargo build --release

%install
install -Dm755 target/release/%{name} %{buildroot}%{_bindir}/%{name}

%files
%license LICENSE
%doc README.md
%{_bindir}/%{name}

%changelog
