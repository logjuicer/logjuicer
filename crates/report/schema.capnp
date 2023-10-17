@0xf8ebf47bdd7cb069;

struct Property(Value) {
  key   @0 :Text;
  value @1 :Value;
}

struct Report {
  createdAt         @0 :UInt64;
  runTime           @1 :UInt64;
  target            @2 :Content;
  baselines         @3 :List(Content);
  logReports        @4 :List(LogReport);
  indexReports      @5 :List(Property(IndexReport));
  unknownFiles      @6 :List(Property(List(Source)));
  readErrors        @7 :List(ReadError);
  totalLineCount    @8 :UInt32;
  totalAnomalyCount @9 :UInt32;
}

struct Content {
  union {
    file      @0 :Source;
    dir       @1 :Source;
    zuul      @2 :Zuul;
    prow      @3 :Prow;
    localZuul @4 :LocalZuul;
  }

  struct Zuul {
    api      @0  :Text;
    uuid     @1  :Text;
    jobName  @2  :Text;
    project  @3  :Text;
    branch   @4  :Text;
    result   @5  :Text;
    pipeline @6  :Text;
    logUrl   @7  :Text;
    refUrl   @8  :Text;
    endTime  @9  :TimestampInMs;
    change   @10 :UInt64;
  }

  struct Prow {
    url         @0 :Text;
    uid         @1 :Text;
    jobName     @2 :Text;
    project     @3 :Text;
    pr          @4 :UInt64;
    storageType @5 :Text;
    storagePath @6 :Text;
  }

  struct LocalZuul {
    path        @0 :Text;
    build       @1 :Zuul;
  }
}

struct Source {
  union {
    local    @0 :SourceRef;
    remote   @1 :SourceRef;
  }
}

struct SourceRef {
  prefix     @0 :UInt16;
  loc        @1 :Text;
}

struct LogReport {
  testTime   @0 :UInt64;
  lineCount  @1 :UInt32;
  byteCount  @2 :UInt32;
  anomalies  @3 :List(AnomalyContext);
  source     @4 :Source;
  indexName  @5 :Text;
}

struct AnomalyContext {
  before     @0 :List(Text);
  anomaly    @1 :Anomaly;
  after      @2 :List(Text);
}

struct Anomaly {
  distance   @0 :Float32;
  pos        @1 :UInt32;
  line       @2 :Text;
}

struct IndexReport {
  trainTime  @0 :UInt64;
  sources    @1 :List(Source);
}

struct ReadError {
  source     @0 :Source;
  error      @1 :Text;
}

using TimestampInMs = UInt64;
