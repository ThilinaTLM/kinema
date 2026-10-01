# SPDX-License-Identifier: Apache-2.0
ARG BASE=ubuntu:22.04
FROM ${BASE}
COPY install-test-deps.sh /opt/install-test-deps.sh
RUN bash /opt/install-test-deps.sh && rm -rf /var/lib/apt/lists/*
WORKDIR /src
