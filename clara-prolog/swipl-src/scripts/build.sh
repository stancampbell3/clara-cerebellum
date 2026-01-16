#!/usr/bin/bash
cd swipl-devel
mkdir build
cd build
cmake -DCMAKE_INSTALL_PREFIX=/Volumes/T7 Shield/Development/ -DCMAKE_BUILD_TYPE=PGO -G Ninja ..
ninja
ctest -j $(nproc) --output-on-failure
ninja install
