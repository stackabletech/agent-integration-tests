/stackable.sh testdriver-1 -i /.cluster/key 'sudo sh -c "echo \"13.32.25.75     static.rust-lang.org\" >> /etc/hosts"'
/stackable.sh testdriver-1 -i /.cluster/key 'curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y'
/stackable.sh testdriver-1 -i /.cluster/key 'cargo --version'
/stackable.sh testdriver-1 -i /.cluster/key 'sudo yum install vim procps curl gcc make pkgconfig openssl-devel systemd-devel python3-pip container-selinux selinux-policy-base git -y'
/stackable.sh testdriver-1 -i /.cluster/key "git clone -b $GIT_BRANCH https://github.com/stackabletech/agent-integration-tests.git"
/stackable.sh testdriver-1 -i /.cluster/key 'cd agent-integration-tests/ && cargo test'
exit_code=$?
/stackable.sh main-1 -i /.cluster/key 'journalctl -u stackable-agent' > /target/stackable-agent.log
exit $exit_code
