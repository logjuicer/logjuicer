// Copyright (C) 2015 The sprs Developers
// Copyright (C) 2024 Red Hat
// SPDX-License-Identifier: Apache-2.0

/// This module provides a new vstack helper that removes removes duplicated rows.
use sprs::*;

/// Stack the given matrices into a new one, using the most efficient stacking
/// direction (ie vertical stack for CSR matrices, horizontal stack for CSC)
pub fn nub_vstack<'a, N, I, Iptr, MatArray>(mats: &MatArray) -> CsMatI<N, I, Iptr>
where
    N: 'a + PartialEq + Clone,
    I: 'a + PartialEq + SpIndex,
    Iptr: 'a + SpIndex,
    MatArray: AsRef<[CsMatViewI<'a, N, I, Iptr>]>,
{
    let mats = mats.as_ref();
    assert!(!mats.is_empty(), "Empty stacking list");
    let inner_dim = mats[0].inner_dims();
    assert!(
        mats.iter().all(|x| x.inner_dims() == inner_dim),
        "Dimension mismatch"
    );
    let storage_type = mats[0].storage();
    assert!(
        mats.iter().all(|x| x.storage() == storage_type),
        "Storage mismatch"
    );

    let outer_dim = mats.iter().map(CsMatBase::outer_dims).sum::<usize>();
    let nnz = mats.iter().map(CsMatBase::nnz).sum::<usize>();

    let mut uniques = Vec::with_capacity(outer_dim);
    let mut res = CsMatI::empty(storage_type, inner_dim);
    res.reserve_outer_dim_exact(outer_dim);
    res.reserve_nnz_exact(nnz);
    for (pos, mat) in mats.iter().enumerate() {
        for vec in mat.outer_iterator() {
            if pos == 0 || uniques.iter().all(|v| v != &vec) {
                res = res.append_outer_csvec(vec.view());
                uniques.push(vec);
            }
        }
    }

    res
}
