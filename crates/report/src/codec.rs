// Copyright (C) 2023 Red Hat
// SPDX-License-Identifier: Apache-2.0

use crate::*;

use capnp::Result;
use std::io::BufRead;
use std::time::{Duration, SystemTime};
use std::{convert::TryInto, ops::Add};

pub struct ReportEncoder;

impl Default for ReportEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl ReportEncoder {
    pub fn new() -> Self {
        // TODO: Intern IndexName
        Self
    }

    pub fn encode(&self, report: &Report, write: impl capnp::io::Write) -> Result<()> {
        let mut message = capnp::message::Builder::new_default();
        let mut module = message.init_root::<schema_capnp::report::Builder>();

        module.set_created_at(write_system_time(&report.created_at)?);
        module.set_run_time(write_duration(&report.run_time)?);
        self.write_content(&report.target, module.reborrow().init_target())?;
        {
            let mut builder = module
                .reborrow()
                .init_baselines(report.baselines.len() as u32);
            for (idx, content) in report.baselines.iter().enumerate() {
                let content_builder = builder.reborrow().get(idx as u32);
                self.write_content(content, content_builder)?;
            }
        }
        {
            let mut builder = module
                .reborrow()
                .init_log_reports(report.log_reports.len() as u32);
            for (idx, log_report) in report.log_reports.iter().enumerate() {
                let mut log_report_builder = builder.reborrow().get(idx as u32);
                self.write_log_report(log_report, &mut log_report_builder)?;
            }
        }
        {
            let mut builder = module
                .reborrow()
                .init_index_reports(report.index_reports.len() as u32);
            for (idx, (index, index_report)) in report.index_reports.iter().enumerate() {
                let mut property = builder.reborrow().get(idx as u32);
                property.set_key(index.as_str().into());
                self.write_index_report(index_report, property.init_value())?;
            }
        }
        {
            let mut builder = module
                .reborrow()
                .init_unknown_files(report.unknown_files.len() as u32);
            for (idx, (index, sources)) in report.unknown_files.iter().enumerate() {
                let mut property = builder.reborrow().get(idx as u32);
                property.set_key(index.as_str().into());
                self.write_sources(sources, property.initn_value(sources.len() as u32))?;
            }
        }
        {
            let mut builder = module
                .reborrow()
                .init_read_errors(report.read_errors.len() as u32);
            for (idx, read_error) in report.read_errors.iter().enumerate() {
                let mut error_builder = builder.reborrow().get(idx as u32);
                error_builder.set_error(read_error.1.as_ref().into());
                self.write_source(&read_error.0, error_builder.init_source())?;
            }
        }
        module.set_total_line_count(report.total_line_count as u32);
        module.set_total_anomaly_count(report.total_anomaly_count as u32);
        capnp::serialize::write_message(write, &message)
    }

    fn write_log_report(
        &self,
        log_report: &LogReport,
        builder: &mut schema_capnp::log_report::Builder,
    ) -> Result<()> {
        builder.set_test_time(write_duration(&log_report.test_time)?);
        builder.set_line_count(log_report.line_count as u32);
        builder.set_byte_count(log_report.byte_count as u32);
        {
            let mut builder = builder
                .reborrow()
                .init_anomalies(log_report.anomalies.len() as u32);
            for (idx, anomaly_context) in log_report.anomalies.iter().enumerate() {
                let mut anomaly_context_builder = builder.reborrow().get(idx as u32);
                self.write_anomaly_context(anomaly_context, &mut anomaly_context_builder)?;
            }
        }
        self.write_source(&log_report.source, builder.reborrow().init_source())?;
        builder.set_index_name(log_report.index_name.as_str().into());
        Ok(())
    }

    fn write_anomaly_context(
        &self,
        anomaly_context: &AnomalyContext,
        builder: &mut schema_capnp::anomaly_context::Builder,
    ) -> Result<()> {
        {
            let mut builder = builder
                .reborrow()
                .init_before(anomaly_context.before.len() as u32);
            for (idx, ctx) in anomaly_context.before.iter().enumerate() {
                builder.set(idx as u32, ctx.as_ref().into());
            }
        }
        self.write_anomaly(
            &anomaly_context.anomaly,
            &mut builder.reborrow().init_anomaly(),
        )?;
        {
            let mut builder = builder
                .reborrow()
                .init_after(anomaly_context.after.len() as u32);
            for (idx, ctx) in anomaly_context.after.iter().enumerate() {
                builder.set(idx as u32, ctx.as_ref().into());
            }
        }
        Ok(())
    }

    fn write_anomaly(
        &self,
        anomaly: &Anomaly,
        builder: &mut schema_capnp::anomaly::Builder,
    ) -> Result<()> {
        // builder.set_distance((255.0 * anomaly.distance) as u8);
        builder.set_distance(anomaly.distance);
        builder.set_pos(anomaly.pos as u32);
        builder.set_line(anomaly.line.as_ref().into());
        Ok(())
    }

    fn write_content(
        &self,
        content: &Content,
        builder: schema_capnp::content::Builder,
    ) -> Result<()> {
        match content {
            Content::File(source) => self.write_source(source, builder.init_file()),
            Content::Directory(source) => self.write_source(source, builder.init_dir()),
            Content::Zuul(build) => self.write_zuul(build, builder.init_zuul()),
            Content::Prow(build) => self.write_prow(build, builder.init_prow()),
            Content::LocalZuulBuild(path, build) => {
                let mut builder = builder.init_local_zuul();
                builder.set_path(
                    path.as_os_str()
                        .to_str()
                        .ok_or(capnp::Error::failed("Bad time".into()))?
                        .into(),
                );
                self.write_zuul(build, builder.init_build())
            }
        }
    }

    fn write_zuul(
        &self,
        zuul: &ZuulBuild,
        mut builder: schema_capnp::content::zuul::Builder,
    ) -> Result<()> {
        builder.set_api(zuul.api.as_str().into());
        builder.set_uuid(zuul.uuid.as_ref().into());
        builder.set_job_name(zuul.job_name.as_ref().into());
        builder.set_project(zuul.project.as_ref().into());
        builder.set_branch(zuul.branch.as_ref().into());
        builder.set_result(zuul.result.as_ref().into());
        builder.set_pipeline(zuul.pipeline.as_ref().into());
        builder.set_log_url(zuul.log_url.as_str().into());
        builder.set_ref_url(zuul.ref_url.as_str().into());
        builder.set_end_time(write_datetime(&zuul.end_time)?);
        builder.set_change(zuul.change);
        Ok(())
    }

    fn write_prow(
        &self,
        prow: &ProwBuild,
        mut builder: schema_capnp::content::prow::Builder,
    ) -> Result<()> {
        builder.set_url(prow.url.as_str().into());
        builder.set_uid(prow.uid.as_ref().into());
        builder.set_job_name(prow.job_name.as_ref().into());
        builder.set_project(prow.project.as_ref().into());
        builder.set_pr(prow.pr);
        builder.set_storage_type(prow.storage_type.as_ref().into());
        builder.set_storage_path(prow.storage_path.as_ref().into());
        Ok(())
    }

    fn write_source(&self, source: &Source, builder: schema_capnp::source::Builder) -> Result<()> {
        match source {
            Source::Local(prefix, path) => {
                let mut builder = builder.init_local();
                builder.set_prefix(
                    (*prefix)
                        .try_into()
                        .map_err(|_| capnp::Error::failed("Bad prefix".into()))?,
                );
                builder.set_loc(
                    path.as_os_str()
                        .to_str()
                        .ok_or(capnp::Error::failed("Bad time".into()))?
                        .into(),
                )
            }
            Source::Remote(prefix, url) => {
                let mut builder = builder.init_remote();
                builder.set_prefix(
                    (*prefix)
                        .try_into()
                        .map_err(|_| capnp::Error::failed("Bad prefix".into()))?,
                );
                builder.set_loc(url.as_str().into())
            }
        };

        Ok(())
    }

    fn write_sources(
        &self,
        sources: &[Source],
        mut builder: capnp::struct_list::Builder<schema_capnp::source::Owned>,
    ) -> Result<()> {
        for (idx, source) in sources.iter().enumerate() {
            let source_builder = builder.reborrow().get(idx as u32);
            self.write_source(source, source_builder)?;
        }
        Ok(())
    }

    fn write_index_report(
        &self,
        index_report: &IndexReport,
        mut builder: schema_capnp::index_report::Builder,
    ) -> Result<()> {
        builder.set_train_time(write_duration(&index_report.train_time)?);
        {
            let builder = builder.init_sources(index_report.sources.len() as u32);
            self.write_sources(&index_report.sources, builder)?;
        }
        Ok(())
    }
}

pub struct ReportDecoder;

impl Default for ReportDecoder {
    fn default() -> Self {
        Self::new()
    }
}

macro_rules! read_hashmap {
    ($reader:expr, $self:expr, $method:ident) => {{
        let reader = $reader;
        let mut map = HashMap::with_capacity(reader.len() as usize);
        for prop in reader.into_iter() {
            let name: &str = prop.get_key()?.to_str()?;
            let values = $self.$method(&prop.get_value()?.into())?;
            let _ = map.insert(IndexName(name.into()), values);
        }
        map
    }};
}

impl ReportDecoder {
    pub fn new() -> Self {
        // TODO: Intern IndexName
        Self
    }

    pub fn decode(&self, reader: impl BufRead) -> Result<Report> {
        let message_reader =
            capnp::serialize::read_message(reader, capnp::message::ReaderOptions::new())?;
        let reader = message_reader.get_root::<schema_capnp::report::Reader<'_>>()?;

        Ok(Report {
            created_at: read_system_time(reader.get_created_at())
                .ok_or(capnp::Error::failed("Bad time".into()))?,
            run_time: read_duration(reader.get_run_time()),
            target: self.read_content(&reader.get_target()?)?,
            baselines: self.read_baselines(&reader.get_baselines()?)?,
            log_reports: self.read_log_reports(&reader.get_log_reports()?)?,
            index_reports: read_hashmap!(reader.get_index_reports()?, self, read_index_report),
            unknown_files: read_hashmap!(reader.get_unknown_files()?, self, read_sources),
            read_errors: self.read_errors(&reader.get_read_errors()?)?,
            total_line_count: reader.get_total_line_count() as usize,
            total_anomaly_count: reader.get_total_anomaly_count() as usize,
        })
    }

    fn read_baselines(
        &self,
        reader: &capnp::struct_list::Reader<schema_capnp::content::Owned>,
    ) -> Result<Vec<Content>> {
        let mut vec = Vec::with_capacity(reader.len() as usize);
        for reader in reader.into_iter() {
            vec.push(self.read_content(&reader)?)
        }
        Ok(vec)
    }

    fn read_log_reports(
        &self,
        reader: &capnp::struct_list::Reader<schema_capnp::log_report::Owned>,
    ) -> Result<Vec<LogReport>> {
        let mut vec = Vec::with_capacity(reader.len() as usize);
        for reader in reader.into_iter() {
            vec.push(self.read_log_report(&reader)?)
        }
        Ok(vec)
    }

    fn read_log_report(&self, reader: &schema_capnp::log_report::Reader) -> Result<LogReport> {
        Ok(LogReport {
            test_time: read_duration(reader.get_test_time()),
            line_count: reader.get_line_count() as usize,
            byte_count: reader.get_byte_count() as usize,
            anomalies: self.read_anomalies(&reader.get_anomalies()?)?,
            source: self.read_source(&reader.get_source()?)?,
            index_name: IndexName(reader.get_index_name()?.to_str()?.into()),
        })
    }

    fn read_anomalies(
        &self,
        reader: &capnp::struct_list::Reader<schema_capnp::anomaly_context::Owned>,
    ) -> Result<Vec<AnomalyContext>> {
        let mut vec = Vec::with_capacity(reader.len() as usize);
        for reader in reader.into_iter() {
            vec.push(self.read_anomaly_context(&reader)?)
        }
        Ok(vec)
    }

    fn read_anomaly_context(
        &self,
        reader: &schema_capnp::anomaly_context::Reader,
    ) -> Result<AnomalyContext> {
        Ok(AnomalyContext {
            before: self.read_context(&reader.get_before()?)?,
            anomaly: self.read_anomaly(&reader.get_anomaly()?)?,
            after: self.read_context(&reader.get_after()?)?,
        })
    }

    fn read_context(&self, reader: &capnp::text_list::Reader) -> Result<Vec<Rc<str>>> {
        let mut vec = Vec::with_capacity(reader.len() as usize);
        for reader in reader.into_iter() {
            vec.push(reader?.to_str()?.into())
        }
        Ok(vec)
    }

    fn read_anomaly(&self, reader: &schema_capnp::anomaly::Reader) -> Result<Anomaly> {
        Ok(Anomaly {
            // distance: (1.0 / 255.0) * reader.get_distance() as f32,
            distance: reader.get_distance(),
            pos: reader.get_pos() as usize,
            line: reader.get_line()?.to_str()?.into(),
        })
    }

    fn read_content(&self, reader: &schema_capnp::content::Reader) -> Result<Content> {
        use schema_capnp::content::Which;
        Ok(match reader.which()? {
            Which::File(reader) => Content::File(self.read_source(&reader?)?),
            Which::Dir(reader) => Content::Directory(self.read_source(&reader?)?),
            Which::Zuul(reader) => Content::Zuul(Box::new(self.read_zuul(&reader?)?)),
            Which::Prow(reader) => Content::Prow(Box::new(self.read_prow(&reader?)?)),
            Which::LocalZuul(reader) => {
                let reader = reader?;
                let path = reader.get_path()?.to_str()?.into();
                Content::LocalZuulBuild(path, Box::new(self.read_zuul(&reader.get_build()?)?))
            }
        })
    }

    fn read_zuul(&self, reader: &schema_capnp::content::zuul::Reader) -> Result<ZuulBuild> {
        Ok(ZuulBuild {
            api: ApiUrl::parse(reader.get_api()?.to_str()?).unwrap(),
            uuid: reader.get_uuid()?.to_str()?.into(),
            job_name: reader.get_job_name()?.to_str()?.into(),
            project: reader.get_project()?.to_str()?.into(),
            branch: reader.get_branch()?.to_str()?.into(),
            result: reader.get_result()?.to_str()?.into(),
            pipeline: reader.get_pipeline()?.to_str()?.into(),
            log_url: read_url(reader.get_log_url()?)?,
            ref_url: read_url(reader.get_ref_url()?)?,
            end_time: read_datetime(reader.get_end_time())?,
            change: reader.get_change(),
        })
    }

    fn read_prow(&self, reader: &schema_capnp::content::prow::Reader) -> Result<ProwBuild> {
        Ok(ProwBuild {
            url: read_url(reader.get_url()?)?,
            uid: reader.get_uid()?.to_str()?.into(),
            job_name: reader.get_job_name()?.to_str()?.into(),
            project: reader.get_project()?.to_str()?.into(),
            pr: reader.get_pr(),
            storage_type: reader.get_storage_type()?.to_str()?.into(),
            storage_path: reader.get_storage_path()?.to_str()?.into(),
        })
    }

    fn read_source(&self, reader: &schema_capnp::source::Reader) -> Result<Source> {
        use schema_capnp::source::Which;
        Ok(match reader.which()? {
            Which::Local(reader) => {
                let reader = reader?;
                Source::Local(
                    reader.get_prefix().into(),
                    reader.get_loc()?.to_str()?.into(),
                )
            }
            Which::Remote(reader) => {
                let reader = reader?;
                Source::Remote(reader.get_prefix().into(), read_url(reader.get_loc()?)?)
            }
        })
    }

    fn read_sources(
        &self,
        reader: &capnp::struct_list::Reader<schema_capnp::source::Owned>,
    ) -> Result<Vec<Source>> {
        let mut vec = Vec::with_capacity(reader.len() as usize);
        for reader in reader.into_iter() {
            vec.push(self.read_source(&reader)?);
        }
        Ok(vec)
    }

    fn read_index_report(
        &self,
        reader: &schema_capnp::index_report::Reader,
    ) -> Result<IndexReport> {
        Ok(IndexReport {
            train_time: read_duration(reader.get_train_time()),
            sources: self.read_sources(&reader.get_sources()?)?,
        })
    }
    fn read_errors(
        &self,
        reader: &capnp::struct_list::Reader<schema_capnp::read_error::Owned>,
    ) -> Result<Vec<(Source, Box<str>)>> {
        let mut vec = Vec::with_capacity(reader.len() as usize);
        for reader in reader.into_iter() {
            let src = self.read_source(&reader.get_source()?)?;
            let err = reader.get_error()?.to_str()?.into();
            vec.push((src, err));
        }
        Ok(vec)
    }
}

fn read_url(reader: capnp::text::Reader) -> Result<url::Url> {
    url::Url::parse(reader.to_str()?).map_err(|_| capnp::Error::failed("Bad url".into()))
}

pub(crate) fn read_datetime(ts: u64) -> Result<DateTime<Utc>> {
    let its: i64 = ts
        .try_into()
        .map_err(|_| capnp::Error::failed("Timestamp can't be converted to i64".into()))?;
    Ok(DateTime::<Utc>::UNIX_EPOCH.add(chrono::Duration::milliseconds(its)))
}

fn write_datetime(dt: &DateTime<Utc>) -> Result<u64> {
    let its: i64 = dt.timestamp_millis();
    its.try_into()
        .map_err(|_| capnp::Error::failed("Timestamp can't be converted to u64".into()))
}

fn read_duration(v: u64) -> Duration {
    Duration::from_millis(v)
}

fn read_system_time(v: u64) -> Option<SystemTime> {
    std::time::SystemTime::UNIX_EPOCH.checked_add(read_duration(v))
}

fn write_duration(d: &Duration) -> Result<u64> {
    d.as_millis()
        .try_into()
        .map_err(|_| capnp::Error::failed("Duration is too big".into()))
}

fn write_system_time(t: &SystemTime) -> Result<u64> {
    write_duration(
        &t.duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|_| capnp::Error::failed("Bad time".into()))?,
    )
}

#[test]
fn capnp_roundtrip() {
    let report = Report::sample();
    let mut buffer = std::io::Cursor::new(vec![]);
    ReportEncoder::new().encode(&report, &mut buffer).unwrap();
    buffer.set_position(0);
    let report_back = ReportDecoder::new().decode(buffer).unwrap();
    assert_eq!(report, report_back);
}
