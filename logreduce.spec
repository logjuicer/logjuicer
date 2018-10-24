%{?scl:%scl_package logreduce}
%{!?scl:%global pkg_name %{name}}

Name:           %{?scl_prefix}logreduce
Version:        0.1.0
Release:        2%{?dist}
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
Requires:       %{?scl_prefix}python-requests
Requires:       %{?scl_prefix}python-scikit-learn
Requires:       %{?scl_prefix}PyYAML

%{?scl:Requires: %{scl}-runtime}
%{?scl:BuildRequires: %{scl}-runtime}

%description
Extract anomalies from log files


%package server
Summary:        The logreduce server
Requires:       %{?scl_prefix}logreduce
Requires:       %{?scl_prefix}python-alembic
Requires:       %{?scl_prefix}python-sqlalchemy
Requires:       %{?scl_prefix}python-cherrypy
Requires:       %{?scl_prefix}python-routes
Requires:       %{?scl_prefix}python-voluptuous
Requires:       %{?scl_prefix}python-gear

%description server
The logreduce server


%package worker
Summary:        The logreduce worker
Requires:       %{?scl_prefix}logreduce
Requires:       %{?scl_prefix}python-gear

%description worker
The logreduce worker


%package webui
Summary:        The logreduce web interface
BuildRequires:  patternfly-react-ui-deps

%description webui
The logreduce web interface


%prep
%autosetup -n logreduce-%{version} -p1
rm -Rf requirements.txt test-requirements.txt *.egg-info


%build
%{?scl:scl enable %{scl} - << \EOF}
PBR_VERSION=%{version} %{__python3} setup.py build
%{?scl:EOF}
sed -e 's#/var/lib/logreduce#/var/opt/rh/rh-python35/lib/logreduce#' \
    -e 's#/var/log/logreduce#/var/opt/rh/rh-python35/log/logreduce#' \
    -i etc/logreduce/config.yaml
sed -e 's#/usr/share/#/opt/rh/rh-python35/root/usr/share/#' \
    -i etc/httpd/logreduce.conf
pushd web
ln -s /opt/patternfly-react-ui-deps/node_modules/ node_modules
PUBLIC_URL="/log-classify/" ./node_modules/.bin/yarn build
popd


%install
install -p -d -m 0755 %{buildroot}/%{_datadir}/log-classify
mv web/build/* %{buildroot}/%{_datadir}/log-classify
%{?scl:scl enable %{scl} - << \EOF}
PBR_VERSION=%{version} %{__python3} setup.py install -O1 --skip-build --root %{buildroot}
%{?scl:EOF}
install -p -D -m 0644 etc/systemd/logreduce-server.service %{buildroot}%{_unitdir}/%{?scl_prefix}logreduce-server.service
install -p -D -m 0644 etc/systemd/logreduce-worker.service %{buildroot}%{_unitdir}/%{?scl_prefix}logreduce-worker.service
install -p -D -m 0644 etc/logreduce/config.yaml %{buildroot}%{_sysconfdir}/logreduce/config.yaml
install -p -D -m 0644 etc/httpd/logreduce.conf %{buildroot}/etc/httpd/conf.d/logreduce.conf
install -p -d -m 0700 %{buildroot}%{_sharedstatedir}/logreduce
install -p -d -m 0700 %{buildroot}%{_localstatedir}/log/logreduce
install -p -d -m 0755 %{buildroot}/var/www/logreduce/anomalies
install -p -d -m 0755 %{buildroot}/var/www/logreduce/logs


%pre server
getent group logreduce >/dev/null || groupadd -r logreduce
if ! getent passwd logreduce >/dev/null; then
  useradd -r -g logreduce -G logreduce -d %{_sharedstatedir}/logreduce -s /sbin/nologin -c "Logreduce Daemon" logreduce
fi
exit 0

%pre worker
getent group logreduce >/dev/null || groupadd -r logreduce
if ! getent passwd logreduce >/dev/null; then
  useradd -r -g logreduce -G logreduce -d %{_sharedstatedir}/logreduce -s /sbin/nologin -c "Logreduce Daemon" logreduce
fi
exit 0


%post server
%systemd_post %{?scl_prefix}logreduce-server.service
%post worker
%systemd_post %{?scl_prefix}logreduce-worker.service

%preun server
%systemd_preun %{?scl_prefix}logreduce-server.service
%preun worker
%systemd_preun %{?scl_prefix}logreduce-worker.service

%postun server
%systemd_postun %{?scl_prefix}logreduce-server.service
%postun worker
%systemd_postun %{?scl_prefix}logreduce-worker.service


%files
%license LICENSE
%doc README.rst
%{python3_sitelib}/logreduce*
%{_bindir}/logreduce
%{_bindir}/logreduce-client
%config(noreplace) %attr(0640, root, logreduce) %{_sysconfdir}/logreduce/config.yaml
%dir %attr(0750, logreduce, logreduce) %{_sharedstatedir}/logreduce
%dir %attr(0750, logreduce, logreduce) %{_localstatedir}/log/logreduce

%files server
%{_bindir}/logreduce-server
%config(noreplace) /etc/httpd/conf.d/logreduce.conf
%{_unitdir}/%{?scl_prefix}logreduce-server.service
%dir %attr(0755, logreduce, logreduce) /var/www/logreduce/logs
%dir %attr(0755, logreduce, logreduce) /var/www/logreduce/anomalies

%files worker
%{_bindir}/logreduce-worker
%{_unitdir}/%{?scl_prefix}logreduce-worker.service

%files webui
%{_datadir}/log-classify

%changelog
* Mon Jul  9 2018 Tristan Cacqueray <tdecacqu@redhat.com> - 0.1.0-2
- Add server service

* Fri Mar 02 2018 Tristan Cacqueray <tdecacqu@redhat.com> - 0.1.0-1
- Initial packaging
