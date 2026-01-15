list:
    just -l

sandhole:
    sandhole --domain=foobar.tld \
        --user-keys-directory=./test_data/user_keys \
        --certificates-directory=./test_data/certificates_sandhole \
        --ssh-port=13122 \
        --http-port=13180 \
        --https-port=13133 \
        --allow-requested-subdomains

sish:
    sish --domain foobar.tld \
        --authentication-keys-directory=./test_data/user_keys \
        --https-certificate-directory=./test_data/certificates_sish \
        --private-keys-directory=./test_data/keys_sish \
        --ssh-address=:13222 \
        --http-address=:13280 \
        --https-address=:13233 \
        --https \
        --load-templates=false \
        --bind-random-subdomains=false

service-sandhole:
    cargo run --release -p sandhole-benchmark-service -- \
        --private-key ./test_data/ssh_key \
        --port 13122 \
        localhost

service-sish:
    cargo run --release -p sandhole-benchmark-service -- \
        --private-key ./test_data/ssh_key \
        --port 13222 \
        localhost

measure-sandhole +ARGS:
    cargo run --release -p sandhole-benchmark-measure -- \
        --custom-ca-cert=./test_data/ca/rootCA.pem \
        --host-ip=127.0.0.1:13133 \
        {{ ARGS }} \
        https://measure.foobar.tld:13133

measure-sish +ARGS:
    cargo run --release -p sandhole-benchmark-measure -- \
        --custom-ca-cert=./test_data/ca/rootCA.pem \
        --host-ip=127.0.0.1:13233 \
        {{ ARGS }} \
        https://measure.foobar.tld:13233
