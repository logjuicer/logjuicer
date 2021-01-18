%global with_ui 0

Name:           logreduce
Version:        0.6.0
Release:        1%{?dist}
Summary:        Extract anomalies from log files

License:        ASL 2.0
URL:            http://logreduce.softwarefactory-project.io
Source0:        http://tarball.softwarefactory-project.io/logreduce/logreduce-%{version}.tar.gz

BuildArch:      noarch

BuildRequires:  python3-devel
BuildRequires:  python3-setuptools
BuildRequires:  python3-pbr

Requires:       python3-setuptools
Requires:       python3-pbr
Requires:       python3-aiohttp
Requires:       python3-requests
Requires:       python3-scikit-learn
Requires:       python3-pyyaml
Requires:       python3-typing-extensions

%description
Extract anomalies from log files


%prep
%autosetup -n logreduce-%{version} -p1
rm -Rf requirements.txt test-requirements.txt *.egg-info


%build
export PBR_VERSION=%{version}
%py3_build


%install
install -p -d -m 0755 %{buildroot}/%{_datadir}/log-classify
export PBR_VERSION=%{version}
%py3_install


%files
%license LICENSE
%doc README.rst
%{python3_sitelib}/logreduce*
%{_bindir}/logreduce

%changelog
* Fri Oct 30 2020 Tristan Cacqueray <tdecacqu@redhat.com> - 0.6.0-1
- Remove server components

* Tue Dec 17 2019 Tristan Cacqueray <tdecacqu@redhat.com> - 0.5.1-1
- Do not build web ui by default

* Wed Sep 25 2019 Tristan Cacqueray <tdecacqu@redhat.com> - 0.5.0-1
- Remove SCL macros

* Mon Jul  9 2018 Tristan Cacqueray <tdecacqu@redhat.com> - 0.1.0-2
- Add server service

* Fri Mar 02 2018 Tristan Cacqueray <tdecacqu@redhat.com> - 0.1.0-1
- Initial packaging
