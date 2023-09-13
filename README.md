# modality-json-plugins

A [Modality][modality] reflector plugin suite and ingest adapter library for JSON lines data.

## Getting Started

1. Write a reflector configuration file that describes how to parse the data
2. Use the importer to import one or more JSON lines files:
  ```
  modality-reflector import json /path/to/data.json
  ```

## Configuration

All of the plugins can be configured through a TOML configuration file (from either the `--config` option or the `MODALITY_REFLECTOR_CONFIG` environment variable).
All of the configuration fields can optionally be overridden at the CLI, see `--help` for more details.

See the [`modality-reflector` Configuration File documentation](https://docs.auxon.io/modality/ingest/modality-reflector-configuration-file.html) for more information
about the reflector configuration.

### Importer Section

These `metadata` fields are specific to the importer plugin.

Note that individual plugin configuration goes in a specific table in your
reflector configuration file, e.g. `[plugins.ingest.importers.json.metadata]`.

* `[plugins.ingest.importers.json.metadata]` — Plugin configuration table. (just `metadata` if running standalone)
  - `run-id` — Use the provided UUID as the run ID instead of generating a random one.
  - `timeout-seconds` — The ingest protocol connection timeout to use.
  - `event-names` — Array of JSON paths to the keys that will be used to determine the
    name of an event. If given multiple times, the paths with be
    checked in order and the first JSON path which exists will be
    used.
  - `timeline-names` — Array of JSON paths to the keys that will be used to determine the
    name (and identity) of a timeline. If given multiple times,
    the paths with be checked in order and the first JSON path
    which exists will be used. The chosen key and value will be
    added to the timeline attrs.
  - `timeline-name-prefix` — Add this string as a prefix to each generated timeline name.
  - `timeline-attrs` — Array of JSON paths to to add as timeline attributes.
  - `[[rename-timeline-attrs]]` — Rename a timeline attribute key as it is being imported.
    * `original` — The attr key to rename.
    * `new` — The new attr key name to use.
  - `[[rename-event-attrs]]` — Rename an event attribute key as it is being imported.
    * `original` — The attr key to rename.
    * `new` — The new attr key name to use.
  - `timestamp-attr` — The JSON path where the event's timestamp can be found.
  - `timestamp-attr-units` — The units of `timestamp-attr`, in the source data. One of s, ms, us, ns.
  - `non-json-regex` — A regex used to parse lines that are not a JSON object.
  - `non-json-attrs` — The name for an attr to use for data extracted from subgroups
    in non-json-regex. These are treated positionally, with
    respect to the subgroup's position in the regex. This data is
    produced early, so all the other options apply equally to the
    regex-extracted data and to the JSON-sourced data.
  - `inputs` — Array of input paths to parse.

### Configuration Example

Given the following example JSON lines data, we can write a reflector configuration
file to extract the [Modality timeline and event](https://docs.auxon.io/modality/concepts.html#events-and-timelines)
attribute information.

```json
{"component": "monitor", "timestamp": 1, "msg": "startup"}
{"component": "sensor", "timestamp": 2, "msg": "measure temperature", "temperature": 55.2}
{"component": "sensor", "timestamp": 3, "msg": "send measurement", "dest": "monitor", "seqnum": 1}
{"component": "monitor", "timestamp": 4, "msg": "recv measurement", "src": "sensor", "seqnum": 1, "temperature": 55.2}
{"component": "monitor", "timestamp": 5, "msg": "report status"}
```

This reflector configuration file will treat each `component` as a separate timeline, where
each `msg` will be an event on that timeline.

```toml
# my-reflector-config.toml
[plugins.ingest.importers.json.metadata]
timeline-names = ['component']
timeline-attrs = ['component']
event-names = ['msg']
timestamp-attr = 'timestamp'
timestamp-attr-units = 's'
```

We can also write a [Modality workspace](https://docs.auxon.io/modality/concepts.html#workspaces-and-segmentation)
configuration file that describes the communications using the declarative interactions syntax.

```toml
# my-workspace.toml
custom-interactions = [
"""
  "send measurement" @ sensor as tx
    -> "recv measurement" @ monitor as rx
  AND tx.seqnum = rx.seqnum
""",
]

[[segmentation-rule]]
name = "by-run-id"
attribute = "run_id"
segment-name-template = "Run {timeline.run_id}"
causally-partition-segments = true
```

We can inspect the data using [`modality log`](https://docs.auxon.io/modality/reference/cli/log.html):

```
■             startup @ monitor   [%5dd4b7062c594bf0acb40decb39ea3dc:00]
║               msg = startup
║               name = startup
║               timestamp = 1000000000
║
║           ■             "measure temperature" @ sensor   [%30a2b5e39b9a47fcb4f9935be94e8d1c:01]
║           ║               msg = measure temperature
║           ║               name = measure temperature
║           ║               temperature = 55.2
║           ║               timestamp = 2000000000
║           ║
║           ○             "send measurement" @ sensor   [%30a2b5e39b9a47fcb4f9935be94e8d1c:02]
║  ╭────────║               [Interaction i0000]
║  │                        dest = monitor
║  │                        msg = send measurement
║  │                        name = send measurement
║  │                        seqnum = 1
║  │                        timestamp = 3000000000
║  │
○  │                      "recv measurement" @ monitor   [%5dd4b7062c594bf0acb40decb39ea3dc:03]
║◀─╯                        [Interaction i0000]
║                           msg = recv measurement
║                           name = recv measurement
║                           seqnum = 1
║                           src = sensor
║                           temperature = 55.2
║                           timestamp = 4000000000
║
■                         "report status" @ monitor   [%5dd4b7062c594bf0acb40decb39ea3dc:04]
                            msg = report status
                            name = report status
                            timestamp = 5000000000
```

## LICENSE

See [LICENSE](./LICENSE) for more details.

Copyright 2022 [Auxon Corporation](https://auxon.io)

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

[http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0)

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.

[modality]: https://auxon.io/products/modality
