//! Helpers for working with predicates.
//!
//! Note: contents of this module are meant to be used via the [`Into`] trait.
//! They are not a part of public API.

use predicates::reflection::{Child, PredicateReflection};
use predicates::Predicate;

/// This trait allows converting tuples of predicates into predicates that
/// accept tuples. That is, if `T = (T1, T2, ...)`, this trait can convert
/// a tuple of predicates `(Predicate<T1>, Predicate<T2>, ...)`
/// into a `Predicate<(T1, T2, ...)>`.
pub trait TuplePredicate<T> {
    /// Concrete implementation of a tuple predicate, depends on tuple length.
    type P: Predicate<T>;

    /// Given that `self` is a tuple of predicates `ps = (p1, p2, ...)`,
    /// returns a predicate that accepts a tuple `ts = (t1, t2, ...)`
    /// and applies predicates element-wise: `ps.0(ts.0) && ps.1(ts.1) && ...`.
    fn into_predicate(self) -> Self::P;
}

pub mod detail {
    use super::*;

    macro_rules! impl_tuple_predicate {
        ($name: ident, $count: expr, $( $t: ident : $p: ident : $n: tt, )*) => {
            pub struct $name<$($t, $p: Predicate<$t>, )*>(($($p, )*), std::marker::PhantomData<($($t, )*)>);

            impl<$($t, $p: Predicate<$t>, )*> std::fmt::Display for $name<$($t, $p, )*> {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, "element-wise tuple predicate")
                }
            }

            impl<$($t, $p: Predicate<$t>, )*> PredicateReflection for $name<$($t, $p, )*> {
                fn children(&self) -> Box<dyn Iterator<Item=Child<'_>> + '_> {
                    let params = vec![$(predicates::reflection::Child::new(stringify!($n), &self.0.$n), )*];
                    Box::new(params.into_iter())
                }
            }

            impl<$($t, $p: Predicate<$t>, )*> Predicate<($($t, )*)> for $name<$($t, $p, )*> {
                #[allow(unused_variables)]
                fn eval(&self, variable: &($($t, )*)) -> bool {
                    $(self.0.$n.eval(&variable.$n) && )* true
                }
            }

            impl<$($t, $p: Predicate<$t>, )*> TuplePredicate<($($t, )*)> for ($($p, )*) {
                type P = $name<$($t, $p, )*>;
                fn into_predicate(self) -> Self::P {
                    $name(self, std::marker::PhantomData)
                }
            }
        }
    }

    impl_tuple_predicate!(TuplePredicate0, 0,);
    impl_tuple_predicate!(TuplePredicate1, 1, T0:P0:0, );
    impl_tuple_predicate!(TuplePredicate2, 2, T0:P0:0, T1:P1:1, );
    impl_tuple_predicate!(TuplePredicate3, 3, T0:P0:0, T1:P1:1, T2:P2:2, );
    impl_tuple_predicate!(TuplePredicate4, 4, T0:P0:0, T1:P1:1, T2:P2:2, T3:P3:3, );
    impl_tuple_predicate!(TuplePredicate5, 5, T0:P0:0, T1:P1:1, T2:P2:2, T3:P3:3, T4:P4:4, );
    impl_tuple_predicate!(TuplePredicate6, 6, T0:P0:0, T1:P1:1, T2:P2:2, T3:P3:3, T4:P4:4, T5:P5:5, );
    impl_tuple_predicate!(TuplePredicate7, 7, T0:P0:0, T1:P1:1, T2:P2:2, T3:P3:3, T4:P4:4, T5:P5:5, T6:P6:6, );
    impl_tuple_predicate!(TuplePredicate8, 8, T0:P0:0, T1:P1:1, T2:P2:2, T3:P3:3, T4:P4:4, T5:P5:5, T6:P6:6, T7:P7:7, );
    impl_tuple_predicate!(TuplePredicate9, 9, T0:P0:0, T1:P1:1, T2:P2:2, T3:P3:3, T4:P4:4, T5:P5:5, T6:P6:6, T7:P7:7, T8:P8:8, );
    impl_tuple_predicate!(TuplePredicate10, 10, T0:P0:0, T1:P1:1, T2:P2:2, T3:P3:3, T4:P4:4, T5:P5:5, T6:P6:6, T7:P7:7, T8:P8:8, T9:P9:9, );
    impl_tuple_predicate!(TuplePredicate11, 11, T0:P0:0, T1:P1:1, T2:P2:2, T3:P3:3, T4:P4:4, T5:P5:5, T6:P6:6, T7:P7:7, T8:P8:8, T9:P9:9, T10:P10:10, );
    impl_tuple_predicate!(TuplePredicate12, 12, T0:P0:0, T1:P1:1, T2:P2:2, T3:P3:3, T4:P4:4, T5:P5:5, T6:P6:6, T7:P7:7, T8:P8:8, T9:P9:9, T10:P10:10, T11:P11:11, );
    impl_tuple_predicate!(TuplePredicate13, 13, T0:P0:0, T1:P1:1, T2:P2:2, T3:P3:3, T4:P4:4, T5:P5:5, T6:P6:6, T7:P7:7, T8:P8:8, T9:P9:9, T10:P10:10, T11:P11:11, T12:P12:12, );
    impl_tuple_predicate!(TuplePredicate14, 14, T0:P0:0, T1:P1:1, T2:P2:2, T3:P3:3, T4:P4:4, T5:P5:5, T6:P6:6, T7:P7:7, T8:P8:8, T9:P9:9, T10:P10:10, T11:P11:11, T12:P12:12, T13:P13:13, );
    impl_tuple_predicate!(TuplePredicate15, 15, T0:P0:0, T1:P1:1, T2:P2:2, T3:P3:3, T4:P4:4, T5:P5:5, T6:P6:6, T7:P7:7, T8:P8:8, T9:P9:9, T10:P10:10, T11:P11:11, T12:P12:12, T13:P13:13, T14:P14:14, );
    impl_tuple_predicate!(TuplePredicate16, 16, T0:P0:0, T1:P1:1, T2:P2:2, T3:P3:3, T4:P4:4, T5:P5:5, T6:P6:6, T7:P7:7, T8:P8:8, T9:P9:9, T10:P10:10, T11:P11:11, T12:P12:12, T13:P13:13, T14:P14:14, T15:P15:15, );
}
