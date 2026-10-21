#!/bin/sh
set -eu

readonly data_dir=/data
readonly options_file=/data/options.json
readonly token_file=/data/workload-token
readonly engine_config=/data/engine-options.json
. /usr/local/lib/vistoda-app-bootstrap

umask 077
vistoda_require_supervisor_token
vistoda_prepare_data_dir bridge:bridge "${data_dir}"
legacy_token="$(jq -r '.token // empty' "${options_file}")"
vistoda_ensure_hex_token "${token_file}" bridge:bridge "${legacy_token}"
jq -n --rawfile token "${token_file}" \
    '{token: ($token | gsub("\\s"; ""))}' >"${engine_config}"
chmod 0600 "${engine_config}"
chown bridge:bridge "${engine_config}"
vistoda_secure_file bridge:bridge "${data_dir}/provider.sealed"

vistoda_start_child su-exec bridge:bridge vistoda-blink-engine --config "${engine_config}"
vistoda_wait_for_health http://127.0.0.1:8099/healthz 30 1

# discovery-begin: the kit adds url (http://HOSTNAME:8099) and managed_app: true.
jq -n --rawfile token "${token_file}" '{token: ($token | gsub("\\s"; ""))}' |
    vistoda_publish_app_discovery blink_live_bridge 8099
# discovery-end

vistoda_wait_child
