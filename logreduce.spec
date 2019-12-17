%global with_ui 0

Name:           logreduce
Version:        0.5.1
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

%description
Extract anomalies from log files


%package server
Summary:        The logreduce server
Requires:       logreduce = %version
Requires:       python3-alembic
Requires:       python3-sqlalchemy
Requires:       python3-cherrypy
Requires:       python3-routes
Requires:       python3-voluptuous
Requires:       python3-gear

%description server
The logreduce server


%package worker
Summary:        The logreduce worker
Requires:       logreduce = %version
Requires:       python3-gear

%description worker
The logreduce worker

%if %{with_ui}
%package webui
Summary:        The logreduce web interface
BuildRequires:  patternfly-react-ui-deps

%description webui
The logreduce web interface
%endif

%package mqtt
Summary:        The logreduce mqtt client
Requires:       logreduce = %version
Requires:       python3-paho-mqtt

%description mqtt
The logreduce mqtt client


%prep
%autosetup -n logreduce-%{version} -p1
rm -Rf requirements.txt test-requirements.txt *.egg-info


%build
export PBR_VERSION=%{version}
%py3_build

%if %{with_ui}
pushd web
ln -s /opt/patternfly-react-ui-deps/node_modules/ node_modules
PUBLIC_URL="/log-classify/" ./node_modules/.bin/yarn build
popd
%endif

%install
install -p -d -m 0755 %{buildroot}/%{_datadir}/log-classify
export PBR_VERSION=%{version}
%py3_install

%if %{with_ui}
mv web/build/* %{buildroot}/%{_datadir}/log-classify
%endif

install -p -D -m 0644 etc/systemd/logreduce-server.service %{buildroot}%{_unitdir}/logreduce-server.service
install -p -D -m 0644 etc/systemd/logreduce-worker.service %{buildroot}%{_unitdir}/logreduce-worker.service
install -p -D -m 0644 etc/systemd/logreduce-mqtt.service %{buildroot}%{_unitdir}/logreduce-mqtt.service
install -p -D -m 0644 etc/logreduce/config.yaml %{buildroot}%{_sysconfdir}/logreduce/config.yaml
install -p -D -m 0644 etc/httpd/log-classify.conf %{buildroot}/etc/httpd/conf.d/log-classify.conf
install -p -d -m 0700 %{buildroot}%{_sharedstatedir}/logreduce
install -p -d -m 0700 %{buildroot}%{_localstatedir}/log/logreduce
install -p -d -m 0755 %{buildroot}/var/www/log-classify/anomalies
install -p -d -m 0755 %{buildroot}/var/www/log-classify/logs


%pre
getent group logreduce >/dev/null || groupadd -r logreduce
getent passwd logreduce >/dev/null || \
  useradd -r -g logreduce -G logreduce -d %{_sharedstatedir}/logreduce -s /sbin/nologin -c "Logreduce Daemon" logreduce

%post server
%systemd_post logreduce-server.service
%post worker
%systemd_post logreduce-worker.service
%post mqtt
%systemd_post logreduce-mqtt.service

%preun server
%systemd_preun logreduce-server.service
%preun worker
%systemd_preun logreduce-worker.service
%preun mqtt
%systemd_preun logreduce-mqtt.service

%postun server
%systemd_postun logreduce-server.service
%postun worker
%systemd_postun logreduce-worker.service
%postun mqtt
%systemd_postun logreduce-mqtt.service


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
%config(noreplace) /etc/httpd/conf.d/log-classify.conf
%{_unitdir}/logreduce-server.service
%dir %attr(0755, logreduce, logreduce) /var/www/log-classify/logs
%dir %attr(0755, logreduce, logreduce) /var/www/log-classify/anomalies

%files worker
%{_bindir}/logreduce-worker
%{_unitdir}/logreduce-worker.service

%files mqtt
%{_bindir}/logreduce-mqtt
%{_unitdir}/logreduce-mqtt.service

%if %{with_ui}
%files webui
%{_datadir}/log-classify
%endif

%changelog
* Tue Dec 17 2019 Tristan Cacqueray <tdecacqu@redhat.com> - 0.5.1-1
- Do not build web ui by default

* Wed Sep 25 2019 Tristan Cacqueray <tdecacqu@redhat.com> - 0.5.0-1
- Remove SCL macros

* Mon Jul  9 2018 Tristan Cacqueray <tdecacqu@redhat.com> - 0.1.0-2
- Add server service

* Fri Mar 02 2018 Tristan Cacqueray <tdecacqu@redhat.com> - 0.1.0-1
- Initial packaging
