# Copyright (C) 2022 Red Hat
# SPDX-License-Identifier: Apache-2.0

from setuptools import setup
from setuptools_rust import Binding, RustExtension

setup(
    name="logreduce-tokenizer",
    version="1.0",
    rust_extensions=[RustExtension("logreduce_tokenizer", binding=Binding.PyO3)],
    # rust extensions are not zip safe, just like C-extensions.
    zip_safe=False,
)
