use ark_ff::FftField;
use ark_ff::Field;
use ark_ff::Zero;
use ark_poly::domain::Radix2EvaluationDomain;
use ark_poly::EvaluationDomain;
#[cfg(feature = "parallel")]
use rayon::prelude::*;
use std::ops::Add;
use std::time::Instant;

pub struct Timer<'a> {
    name: &'a str,
    start: Instant,
}

impl<'a> Timer<'a> {
    pub fn new(name: &'a str) -> Timer<'a> {
        let start = Instant::now();
        Timer { name, start }
    }
}

impl<'a> Drop for Timer<'a> {
    fn drop(&mut self) {
        println!("{} in {:?}", self.name, self.start.elapsed());
    }
}

pub fn interleave<T: Copy + Send + Sync + Default, const RADIX: usize>(
    source: &[T],
) -> Vec<[T; RADIX]> {
    let n = source.len() / RADIX;
    let mut res = vec![[T::default(); RADIX]; n];
    ark_std::cfg_iter_mut!(res)
        .enumerate()
        .for_each(|(i, element)| {
            for j in 0..RADIX {
                element[j] = source[i + j * n]
            }
        });
    res
}

// pub(crate) fn print_row<F: Field>(row: &[F]) {
//     for val in row {
//         print!("{val}, ");
//     }
//     println!()
// }

/// Rounds the input value up the the nearest power of two
pub fn ceil_power_of_two(value: usize) -> usize {
    if value.is_power_of_two() {
        value
    } else {
        value.next_power_of_two()
    }
}

// from arkworks
/// This evaluates the vanishing polynomial for this domain at tau.
pub fn evaluate_vanishing_polynomial<F: FftField, T: Field>(
    domain: &Radix2EvaluationDomain<F>,
    tau: T,
) -> T
where
    F: Into<T>,
{
    tau.pow([domain.size() as u64]) - domain.coset_offset_pow_size().into()
}

// Evaluates the vanishing polynomial for `vanish_domain` over `eval_domain`
// E.g. evaluates `(x - v_0)(x - v_1)...(x - v_n-1)` over `eval_domain`
pub fn fill_vanishing_polynomial<F: FftField>(
    dst: &mut [F],
    vanish_domain: &Radix2EvaluationDomain<F>,
    eval_domain: &Radix2EvaluationDomain<F>,
) {
    let n = vanish_domain.size();
    let scaled_eval_offset = eval_domain.coset_offset().pow([n as u64]);
    let scaled_eval_generator = eval_domain.group_gen().pow([n as u64]);
    let scaled_vanish_offset = vanish_domain.coset_offset_pow_size();

    #[cfg(feature = "parallel")]
    let chunk_size = std::cmp::max(n / rayon::current_num_threads(), 1024);
    #[cfg(not(feature = "parallel"))]
    let chunk_size = n;

    ark_std::cfg_chunks_mut!(dst, chunk_size)
        .enumerate()
        .for_each(|(i, chunk)| {
            let mut acc = scaled_eval_offset * scaled_eval_generator.pow([(i * chunk_size) as u64]);
            chunk.iter_mut().for_each(|coeff| {
                *coeff = acc - scaled_vanish_offset;
                acc *= &scaled_eval_generator
            })
        });
}

// taken from arkworks-rs
/// Horner's method for polynomial evaluation
#[inline]
pub fn horner_evaluate<F: Field, T: Field>(poly_coeffs: &[F], point: &T) -> T
where
    T: for<'a> Add<&'a F, Output = T>,
{
    poly_coeffs
        .iter()
        .rfold(T::zero(), move |result, coeff| result * point + coeff)
}

// calculates `p / (x^a - b)` using synthetic division
// https://en.wikipedia.org/wiki/Synthetic_division
// remainder is discarded. code copied from Winterfell STARK
pub fn synthetic_divide<F: Field>(coeffs: &mut [F], a: usize, b: F) {
    assert!(!a.is_zero());
    assert!(!b.is_zero());
    assert!(coeffs.len() > a);
    if a == 1 {
        let mut c = F::zero();
        for coeff in coeffs.iter_mut().rev() {
            *coeff += b * c;
            core::mem::swap(coeff, &mut c);
        }
    } else {
        todo!()
    }
}

const GRINDING_CONTRIBUTION_FLOOR: usize = 80;

// taken from Winterfell
// also https://github.com/starkware-libs/ethSTARK/blob/master/README.md#7-Measuring-Security
// https://eprint.iacr.org/2020/654.pdf section 7.2 for proven security
// TODO: must investigate and confirm all this.
// TODO: determine if
pub fn conjectured_security_level(
    field_bits: usize,
    hash_fn_security: usize,
    lde_blowup_factor: usize,
    trace_len: usize,
    num_fri_quiries: usize,
    grinding_factor: usize,
) -> usize {
    // compute max security we can get for a given field size
    let field_security = field_bits - (lde_blowup_factor * trace_len).trailing_zeros() as usize;

    // compute security we get by executing multiple query rounds
    let security_per_query = lde_blowup_factor.ilog2() as usize;
    let mut query_security = security_per_query * num_fri_quiries;

    // include grinding factor contributions only for proofs adequate security
    if query_security >= GRINDING_CONTRIBUTION_FLOOR {
        query_security += grinding_factor;
    }

    std::cmp::min(
        std::cmp::min(field_security, query_security) - 1,
        hash_fn_security,
    )
}
