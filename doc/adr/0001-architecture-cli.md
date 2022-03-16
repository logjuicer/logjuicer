# 0001. Architecture of the logreduce-cli

* Status: accepted
* Date: 2022-03-16
* Deciders: Fabien Boucher, Tristan de Cacqueray

The goal of this document is to list the main use-cases of the logreduce
command line interface.

## Context and Problem Statement

Logreduce undergoes a re-implementation and we need to define its scope and API.
We would like to use a flexible architecture to support future goals.

## Considered Options

* [option 1] Focus on the Zuul build use-case.
* [option 2] Define the near future goals and implements the main use-case accordingly.

## Decision Outcome

Chosen option: "[option 2]", because it comes out best (see below).
We leverage the pain-points of the initial implementation to build a flexible architecture.

## Glossary

- Anomaly: a log line that is not found in the baseline.
- Baseline: a nominal content, e.g. a successful build.
- Content: a source of log lines.
- Model: an archive of baselines that is used to search anomaly.
- Report: the list of anomalies found in a target.
- Target: the content to analyze.


## Command Line Use Cases

Logreduce prints the anomalies to the console in real time.
Or it can be used to produce a report to be analyzed later.

### Output Format

Anomalies are displayed like this:

```
file-name:line-number | anomaly
```

Surounding context is also attached without the line-number:

```
scheduler.log      | Processing event for repo ...
scheduler.log:1042 | API rate limit reached
scheduler.log      | Failed to process repo ...
```

In report mode, the file location is preserved so that the full log
can be accessed online.


### Search Local Files

The simplest use-cases is to compare two files:

```ShellSession
$ logreduce diff /var/log/zuul/scheduler.log.1 /var/log/zuul/scheduler.log
```

Or using the baseline discovery rules:

```ShellSession
$ logreduce file /var/log/zuul/scheduler.log
```


### Query JournalD

Investigate a recent outage using journald logs,
for example by comparing with the day before:

```ShellSession
$ logreduce journald now-3hours
```

Or using a range, past outage can also be analyzed:

```ShellSession
$ logreduce journald 2022-03-15T10:15:00 +1hour
```


### Analyze a CI build in post-run action

Look for anomalies in the current build, by using a pre-defined ansible roles, or github action step that would call:

```ShellSession
$ logreduce current-build
```

- Open `~/zuul-info/inventory.yaml` to get the job_name of the current build.
- Lookup pre-existing model, for example in a known location: `${logserver_url}/logreduce-models/${job-name}`.
- If no model can be found, logreduce query the builds API to download the baselines (e.g. previous successfull runs) and build a new model. The newly built model can be uploaded for re-use.
- Search each file group in `~/zuul-output` and collect the anomalies.
- Create a report and attach it to the build result artifacts.

Similarly, a build can be analyzed using it's url:

```ShellSession
$ logreduce url https://build-log-url
```


### Analyze a sosreport case

Given a baseline sosreport, for example produced in a controlled environment, look for anomaly in a report tarball:

```ShellSession
$ logreduce diff baseline-sosreport.tar.gz customer-sosreport.tar.gz
```

Or given a pre-trained model:

```ShellSession
$ logreduce --model ./sosreport.clf file customer-sosreport.tar.gz
```


## Data Types

This section introduces plausible data types for the implementation.
We use the Haskell syntax to describe the types and their associated functions, but the implementation can be done in another language.
This is an initial type driven specification.

### Input

The user Input is defined as:

```haskell
getInput :: Args -> Input

data Input
  = Path Path
  | Url URL
  | Journal Date Duration
  | CurrentBuild
```

### Content

The Input is converted into a target Content:

```haskell
inputToContent :: Input -> IO Content

data Content
  = File Source
  | Directory Source
  | JournalContent Date (Maybe Duration)
  | ZuulBuild ZuulBuildInfo
  | GitHubJob GitHubJobInfo
  | ProwBuild ProwBuildInfo

data Source
  = Local Path
  | Remote URL
  | DevLog

data ZuulBuildInfo = ZuulBuildInfo
  { project :: Text,
    job_name :: Text,
    log_url :: URL,
    ...
  }

-- | Directory sources are relatives:
dirSources :: Source -> [Source]

-- | A zuul build preserves the log_url:
zuulSources :: ZuulBuildInfo -> [Source]
```

Here are the main convertion rules for `inputToContent`:

- Path and URL can be a file or a directory.
- URL that contains specifics path such as `zuul/build/$uuid` or `github.com/$repo/runs/$id` are treated differently. These URLs can easily be copy-pasted from the code review web interface.
- When the input is a directory, if it contains a `zuul-info/inventory.yaml` file then it is converted to a ZuulBuild.
- CurrentBuild looks-up the environment to create the ZuulBuildInfo or GitHubJobInfo record.


### Model

The model is an archive of baselines that is used to search anomaly:

```haskell
getBaselines :: Content  -> IO Baselines
createModel :: Baselines -> IO Model

type Baselines = [Content]

data Model = Model
  { version :: Integer,
    created_at :: Date,
    baselines :: Baselines,
    indexes :: Indexes
  }
```

Here are the main baseline discovery rules of `getBaselines`:

- File baselines may be found in the same directory. For example, "/var/log/zuul/scheduler.log" target may be compared with past files named "scheduler.log.0" or "scheduler.log-YYYY-MM-DD"
- ZuulBuild and GitHubJob baselines may be found by querying the API for previous run.
- JournalContent baselines may be found in a range before the requested Date.

A model may be archived and re-used to avoid fetching the baselines and rebuilding the indexes.


### Indexes

Within a Model, one index is created per source group.

```haskell
createIndex ::   [Source]  -> IO Index
createIndexes :: Baselines -> IO Indexes

data Indexes = Map IndexName Index

sourceToIndexName :: Source -> IndexName
```

Here are the convertion rules for `sourceToIndexName`:

- "scheduler.log" and "scheduler.log.0" gets the same IndexName.
- "k8s_scheduler-afed81.log" and "k8s_scheduler-ac421be.log" also gets the same IndexName.


### Report

Using a Model and a target Content, logreduce produces this output:

```haskell
searchAnomaly :: Index -> Source -> IO [Anomaly]

data Anomaly = Anomaly
  { line_number :: Int,
    anomaly :: Text,
    score :: Float,
    context :: [Text]
  }

createReport :: Model -> Content -> IO Report

data Report = Report
  { baselines :: [Content],
    target    :: Content,
    anomalies :: [(Source, [Anomaly])]
  }
```


## Report minification

An emergent feature for logreduce is to be able to compare multiple reports to find similarities and extract unique errors:

```haskell
minimizeReports :: [Report] -> [(Source, [Anomaly])]
```

This is particularly useful for third party CI which are only interested in new failures.
This can also be useful for gating CI, where a developper might only want to know about the new issues occuring during the development of a change.

How to implement this function remains to be defined. In particular, we need to demonstrate such function:

```haskell
-- | Get the list of unique anomalies
diffReport :: Report -> Report -> [(Source, [Anomaly])]
```

We may want to tolerate small variation between reports.
When looking for anomalies, we check every target with every baseline.
When looking for report similarity, we need to check if every known anomalies are present in the target report.

This feature is a key requirement for the logreduce-service described in the ADR 0002.
