use std::str::FromStr;

macro_rules! make_enum {
    (
        $name:ident =>
            $(
                $key:ident as $value:expr,
            )*
    ) => {
        #[derive(Debug, Clone, PartialEq)]
        pub enum $name {
            $($key),*
        }

        impl FromStr for $name {
            type Err = ();

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($value => Ok($name::$key),)*
                    _ => Err(())
                }
            }
        }

        #[allow(clippy::to_string_trait_impl)]
        impl ToString for $name {
            fn to_string(&self) -> String {
                (match self {
                    $($name::$key => $value,)*
                }).to_string()
            }
        }
    };
}

make_enum! {
    BlockDirection =>
        Up as "up",
        Down as "down",
        North as "north",
        South as "south",
        East as "east",
        West as "west",
}

make_enum! {
    Axis =>
        X as "x",
        Y as "y",
        Z as "z",
}

make_enum! {
    BedPart =>
        Head as "head",
        Foot as "foot",
}

make_enum! {
    Half =>
        Top as "top",
        Bottom as "bottom",
}

make_enum! {
    StairShape =>
        Straight as "straight",
        InnerLeft as "inner_left",
        InnerRight as "inner_right",
        OuterLeft as "outer_left",
        OuterRight as "outer_right",
}

make_enum! {
    BlockType =>
        Single as "single",
        Double as "double",
        Top as "top",
        Bottom as "bottom",
}
