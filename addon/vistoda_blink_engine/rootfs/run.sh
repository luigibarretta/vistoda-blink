#!/bin/sh
set -eu

readonly data_dir=/data
readonly options_file=/data/options.json
readonly token_file=/data/workload-token
readonly engine_config=/data/engine-options.json

umask 077
chown bridge:bridge "${data_dir}"
legacy_token="$(jq -r '.token // empty' "${options_file}")"
if ! test -f "${token_file}" || ! grep -Eq '^[0-9a-f]{64}$' "${token_file}"; then
    if printf '%s' "${legacy_token}" | grep -Eq '^[0-9a-f]{64}$'; then
        printf '%s' "${legacy_token}" >"${token_file}"
    else
        od -An -N32 -tx1 /dev/urandom | tr -d ' \n' >"${token_file}"
    fi
fi
jq -n --rawfile token "${token_file}" \
    '{token: ($token | gsub("\\s"; ""))}' >"${engine_config}"
chmod 0600 "${token_file}" "${engine_config}"
chown bridge:bridge "${token_file}" "${engine_config}"
if test -e "${data_dir}/provider.sealed"; then
    chown bridge:bridge "${data_dir}/provider.sealed"
    chmod 0600 "${data_dir}/provider.sealed"
fi

su-exec bridge:bridge vistoda-blink-engine --config "${engine_config}" &
child_pid=$!

stop_child() {
    kill -TERM "${child_pid}" 2>/dev/null || true
    wait "${child_pid}" 2>/dev/null || true
}
trap stop_child INT TERM

attempt=0
until curl -fsS --max-time 2 http://127.0.0.1:8099/healthz >/dev/null 2>&1; do
    if ! kill -0 "${child_pid}" 2>/dev/null; then
        wait "${child_pid}"
    fi
    attempt=$((attempt + 1))
    test "${attempt}" -lt 30 || exit 1
    sleep 1
done

test -n "${SUPERVISOR_TOKEN:-}" || exit 1
app_hostname="$(curl -fsS --retry 5 --retry-all-errors \
    -H "Authorization: Bearer ${SUPERVISOR_TOKEN}" \
    http://supervisor/addons/self/info | jq -er '.data.hostname')"
private_url="http://${app_hostname}:8099"
jq -n \
    --arg service blink_live_bridge \
    --arg url "${private_url}" \
    --rawfile token "${token_file}" \
    '{service: $service, config: {url: $url,
      token: ($token | gsub("\\s"; "")), managed_app: true}}' |
    curl -fsS --retry 5 --retry-all-errors \
        -H "Authorization: Bearer ${SUPERVISOR_TOKEN}" \
        -H 'Content-Type: application/json' \
        --data-binary @- http://supervisor/discovery >/dev/null

wait "${child_pid}"
