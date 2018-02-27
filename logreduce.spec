%{?scl:%scl_package logreduce}
%{!?scl:%global pkg_name %{name}}

Name:           %{?scl_prefix}logreduce
Version:        0.1.0
Release:        1%{?dist}
Summary:        Extract anomalies from log files

License:        ASL 2.0
URL:            http://logreduce.softwarefactory-project.io
Source0:        http://tarball.softwarefactory-project.io/logreduce/logreduce-%{version}.tar.gz

BuildArch:      noarch

BuildRequires:  %{?scl_prefix}python-devel
BuildRequires:  %{?scl_prefix}python-setuptools
BuildRequires:  %{?scl_prefix}python-pbr

Requires:       %{?scl_prefix}python-setuptools
Requires:       %{?scl_prefix}python-pbr
Requires:       %{?scl_prefix}python-aiohttp
Requires:       %{?scl_prefix}python-scikit-learn
Requires:       %{?scl_prefix}PyYAML

%{?scl:Requires: %{scl}-runtime}
%{?scl:BuildRequires: %{scl}-runtime}

%description
Extract anomalies from log files

%prep
%autosetup -n logreduce-%{version} -p1
rm -Rf requirements.txt test-requirements.txt *.egg-info

%build
%{?scl:scl enable %{scl} - << \EOF}
PBR_VERSION=%{version} %{__python3} setup.py build
%{?scl:EOF}

%install
%{?scl:scl enable %{scl} - << \EOF}
PBR_VERSION=%{version} %{__python3} setup.py install -O1 --skip-build --root %{buildroot}
%{?scl:EOF}

%files
%license LICENSE
%doc README.rst
%{python3_sitelib}/logreduce*
%{_bindir}/logreduce

%changelog
* Fri Mar 02 2018 Tristan Cacqueray <tdecacqu@redhat.com> - 0.1.0-1
- Initial packaging
