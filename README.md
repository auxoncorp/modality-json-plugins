# modality-json-plugins &emsp; ![ci]

A [Modality][modality] reflector plugin suite and ingest adapter library for JSON data.

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
    checked in order and the first json path which exists will be
    used.
  - `timeline-names` — Array of JSON paths to the keys that will be used to determine the
    name (and identity) of a timeline. If given multiple times,
    the paths with be checked in order and the first json path
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
    regex-extracted data and to the json-sourced data.
  - `inputs` — Array of input paths to parse.

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

[ci]: https://github.com/auxoncorp/modality-json-plugins/workflows/CI/badge.svg
[modality]: https://auxon.io/products/modality
