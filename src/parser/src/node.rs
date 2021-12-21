use {
    crate::{ast::AstIndex, ConstantIndex},
    std::{convert::TryFrom, fmt},
};

/// A parsed node that can be included in the [AST](crate::Ast).
///
/// Nodes refer to each other via [AstIndex]s, see [AstNode](crate::AstNode).
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    /// An Empty node, used for `()` empty expressions
    Empty,

    /// A single expression wrapped in parentheses
    Nested(AstIndex),

    /// An identifer
    Id(ConstantIndex),

    /// A meta identifier, e.g. `@display` or `@test my_test`
    Meta(MetaKeyId, Option<ConstantIndex>),

    /// A lookup node, and optionally the node that follows it in the lookup chain
    Lookup((LookupNode, Option<AstIndex>)), // lookup node, next node

    /// A parentheses-free call on a named id, e.g. `foo 1, 2, 3`
    ///
    /// Calls with parentheses or on temporary values are parsed as Lookups
    NamedCall {
        /// The id of the function to be called
        id: ConstantIndex,
        /// The arguments to pass to the function
        args: Vec<AstIndex>,
    },

    /// The `true` keyword
    BoolTrue,

    /// The `false` keyword
    BoolFalse,

    /// The integer `0`
    Number0,

    /// The integer `1`
    Number1,

    /// An integer literal
    Int(ConstantIndex),

    /// An float literal
    Float(ConstantIndex),

    /// A string literal
    Str(AstString),

    /// A list literal
    ///
    /// e.g. `[foo, bar, 42]`
    List(Vec<AstIndex>),

    /// A tuple literal
    ///
    /// e.g. `(foo, bar, 42)`
    ///
    /// Note that this is also used for implicit tuples, e.g. in `x = 1, 2, 3`
    Tuple(Vec<AstIndex>),

    /// A temporary tuple
    ///
    /// Used in contexts where the result won't be exposed directly to the use, e.g.
    /// `x, y = 1, 2` - here `x` and `y` are indexed from the temporary tuple.
    /// `match foo, bar...` - foo and bar will be stored in a temporary tuple for comparison.
    TempTuple(Vec<AstIndex>),

    /// A range with a defined start and end
    Range {
        /// The start of the range
        start: AstIndex,
        /// The end of the range
        end: AstIndex,
        /// Whether or not the end of the range includes the end value itself
        ///
        /// e.g. `1..10` - a range from 1 up to but not including 10
        /// e.g. `1..=10` - a range from 1 up to and including 10
        inclusive: bool,
    },

    /// A range without a defined end
    RangeFrom {
        /// The start of the range
        start: AstIndex,
    },

    /// A range without a defined start
    RangeTo {
        /// The end of the range
        end: AstIndex,
        /// Whether or not the end of the range includes the end value itself
        inclusive: bool,
    },

    /// The range operator without defined start or end
    ///
    /// Used when indexing a list or tuple, and the full contents are to be returned.
    RangeFull,

    /// A map literal, with a series of keys and values
    ///
    /// Values are optional for inline maps.
    Map(Vec<(MapKey, Option<AstIndex>)>),

    /// The main block node
    ///
    /// Typically all ASTs will have this node at the root.
    MainBlock {
        /// The main block's body as a series of expressions
        body: Vec<AstIndex>,
        /// The number of locally assigned values in the main block
        ///
        /// This tells the compiler how many registers need to be reserved for locally
        /// assigned values.
        local_count: usize,
    },

    /// A block node
    ///
    /// Used for indented blocks that share the context of the frame they're in,
    /// e.g. if expressions, arms in match or switch experssions, loop bodies
    Block(Vec<AstIndex>),

    /// A function node
    Function(Function),

    /// An import expression
    ///
    /// Each import item is defined as a series of [ImportItemNode]s,
    /// e.g. `from foo.fun import baz.bar, caz.car.cax`
    Import {
        /// The series of items to import
        items: Vec<Vec<ImportItemNode>>,
        /// Where the items should be imported from
        ///
        /// An empty list here implies that import without `from` has been used.
        from: Vec<ImportItemNode>,
    },

    /// An assignment expression
    ///
    /// Used for single-assignment, multiple-assignment is represented by [Node::MultiAssign].
    Assign {
        /// The target of the assignment
        target: AssignTarget,
        /// The operator to use, e.g. `=`, `+=`, etc.
        op: AssignOp,
        /// The expression to be assigned
        expression: AstIndex,
    },

    /// A multiple-assignment expression
    ///
    /// e.g. `x, y = foo()`, or `foo, bar, baz = 1, 2, 3`
    MultiAssign {
        /// The targets of the assignment
        targets: Vec<AssignTarget>,
        /// The expression to be assigned
        expression: AstIndex,
    },

    /// A unary operation
    UnaryOp {
        /// The operator to use
        op: AstUnaryOp,
        /// The value used in the operation
        value: AstIndex,
    },

    /// A binary operation
    BinaryOp {
        /// The operator to use
        op: AstBinaryOp,
        /// The "left hand side" of the operation
        lhs: AstIndex,
        /// The "right hand side" of the operation
        rhs: AstIndex,
    },

    /// An if expression
    If(AstIf),

    /// A match expression
    Match {
        /// The expression that will be matched against
        expression: AstIndex,
        /// The series of arms that match against the provided expression
        arms: Vec<MatchArm>,
    },

    /// A switch expression
    Switch(Vec<SwitchArm>),

    /// The `_` operator
    ///
    /// Used as a placeholder for unused function arguments or ignored unpacked values.
    Wildcard,

    /// The `...` operator
    ///
    /// Used when capturing variadic arguments, and when unpacking list values.
    Ellipsis(Option<ConstantIndex>),

    /// A `for` loop
    For(AstFor),

    /// A `loop` expression
    Loop {
        /// The loop's body
        body: AstIndex,
    },

    /// A `while` loop
    While {
        /// The condition for the while loop
        condition: AstIndex,
        /// The body of the while loop
        body: AstIndex,
    },

    /// An `until` expression
    Until {
        /// The condition for the until loop
        condition: AstIndex,
        /// The body of the until loop
        body: AstIndex,
    },

    /// The break keyword
    Break,

    /// The continue keyword
    Continue,

    /// A return expression, with optional return value
    Return(Option<AstIndex>),

    /// A try expression
    Try(AstTry),

    /// A throw expression
    Throw(AstIndex),

    /// A yield expression
    Yield(AstIndex),

    /// A debug expression
    Debug {
        /// The stored string of the debugged expression to be used when printing the result
        expression_string: ConstantIndex,
        /// The expression that should be debugged
        expression: AstIndex,
    },
}

impl Default for Node {
    fn default() -> Self {
        Node::Empty
    }
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Node::*;
        match self {
            Empty => write!(f, "Empty"),
            Nested(_) => write!(f, "Nested"),
            Id(_) => write!(f, "Id"),
            Meta(_, _) => write!(f, "Meta"),
            Lookup(_) => write!(f, "Lookup"),
            BoolTrue => write!(f, "BoolTrue"),
            BoolFalse => write!(f, "BoolFalse"),
            Float(_) => write!(f, "Float"),
            Int(_) => write!(f, "Int"),
            Number0 => write!(f, "Number0"),
            Number1 => write!(f, "Number1"),
            Str(_) => write!(f, "Str"),
            List(_) => write!(f, "List"),
            Tuple(_) => write!(f, "Tuple"),
            TempTuple(_) => write!(f, "TempTuple"),
            Range { .. } => write!(f, "Range"),
            RangeFrom { .. } => write!(f, "RangeFrom"),
            RangeTo { .. } => write!(f, "RangeTo"),
            RangeFull => write!(f, "RangeFull"),
            Map(_) => write!(f, "Map"),
            MainBlock { .. } => write!(f, "MainBlock"),
            Block(_) => write!(f, "Block"),
            Function(_) => write!(f, "Function"),
            NamedCall { .. } => write!(f, "NamedCall"),
            Import { .. } => write!(f, "Import"),
            Assign { .. } => write!(f, "Assign"),
            MultiAssign { .. } => write!(f, "MultiAssign"),
            UnaryOp { .. } => write!(f, "UnaryOp"),
            BinaryOp { .. } => write!(f, "BinaryOp"),
            If(_) => write!(f, "If"),
            Match { .. } => write!(f, "Match"),
            Switch { .. } => write!(f, "Switch"),
            Wildcard => write!(f, "Wildcard"),
            Ellipsis(_) => write!(f, "Ellipsis"),
            For(_) => write!(f, "For"),
            While { .. } => write!(f, "While"),
            Until { .. } => write!(f, "Until"),
            Loop { .. } => write!(f, "Loop"),
            Break => write!(f, "Break"),
            Continue => write!(f, "Continue"),
            Return(_) => write!(f, "Return"),
            Try { .. } => write!(f, "Try"),
            Throw(_) => write!(f, "Throw"),
            Yield { .. } => write!(f, "Yield"),
            Debug { .. } => write!(f, "Debug"),
        }
    }
}

/// A function definition
#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    /// The function's arguments
    pub args: Vec<AstIndex>,
    /// The number of locally assigned values
    ///
    /// Used by the compiler when reserving registers for local values at the start of the frame.
    pub local_count: usize,
    /// Any non-local values that are accessed in the function
    ///
    /// Any ID (or lookup root) that's accessed in a function and which wasn't previously assigned
    /// locally, is either an export or the value needs to be captured. The compiler takes care of
    /// determining if an access is a capture or not at the moment the function is created.
    pub accessed_non_locals: Vec<ConstantIndex>,
    /// The function's body
    pub body: AstIndex,
    /// A flag that indicates if the function has used `self` as its first argument
    pub is_instance_function: bool,
    /// A flag that indicates if the function arguments end with a variadic `...` argument
    pub is_variadic: bool,
    /// A flag that indicates if the function is a generator or not
    ///
    /// The presence of a `yield` expression in the function body will set this to true.
    pub is_generator: bool,
}

/// A string definition
#[derive(Clone, Debug, PartialEq)]
pub struct AstString {
    /// Indicates if single or double quotation marks were used
    pub quotation_mark: QuotationMark,
    /// A series of string nodes
    ///
    /// A string is made up of a series of literals and template expressions,
    /// which are then joined together using a string builder.
    pub nodes: Vec<StringNode>,
}

/// A node in a string definition
#[derive(Clone, Debug, PartialEq)]
pub enum StringNode {
    /// A string literal
    Literal(ConstantIndex),
    /// An expression that should be evaluated and inserted into the string
    Expr(AstIndex),
}

/// A for loop definition
#[derive(Clone, Debug, PartialEq)]
pub struct AstFor {
    /// The optional arguments that capture each iteration's output values
    pub args: Vec<Option<ConstantIndex>>,
    /// The expression that produces an iterable value
    pub iterable: AstIndex,
    /// The body of the for loop
    pub body: AstIndex,
}

/// An if expression definition
#[derive(Clone, Debug, PartialEq)]
pub struct AstIf {
    /// The if expression's condition
    pub condition: AstIndex,
    /// The if expression's `then` branch
    pub then_node: AstIndex,
    /// An optional series of `else if` conditions and branches
    pub else_if_blocks: Vec<(AstIndex, AstIndex)>,
    /// An optional `else` branch
    pub else_node: Option<AstIndex>,
}

/// An operation used in UnaryOp expressions
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(missing_docs)]
pub enum AstUnaryOp {
    Negate,
    Not,
}

/// An operation used in BinaryOp expressions
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(missing_docs)]
pub enum AstBinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Equal,
    NotEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    And,
    Or,
    Pipe,
}

/// A try expression definition
#[derive(Clone, Debug, PartialEq)]
pub struct AstTry {
    /// The block that's wrapped by the try
    pub try_block: AstIndex,
    /// The optional identifier that will receive a caught error
    pub catch_arg: Option<ConstantIndex>,
    /// The try expression's catch block
    pub catch_block: AstIndex,
    /// An optional `finally` block
    pub finally_block: Option<AstIndex>,
}

/// The operation used in an assignment expression
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AssignOp {
    /// +=
    Add,
    /// -=
    Subtract,
    /// *=
    Multiply,
    /// /=
    Divide,
    /// %=
    Modulo,
    /// =
    Equal,
}

/// The scope for an assignment
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scope {
    /// The export scope
    ///
    /// This is used in an `export` expression to assign a value to the module's exports map.
    Export,
    /// The local scope
    ///
    /// This is the default scope used in assignments, producing values that are locally assigned.
    Local,
}

/// A node in a lookup chain
///
/// Lookups are any expressions that access a values from identifiers, and then as the lookup chain
/// continues, from any following temporary results.
///
/// In other words, some series of operations involving indexing, `.` accesses, and function calls.
///
/// e.g.
/// `foo.bar."baz"[0](42)`
///  |  |   |     |  ^ Call {args: 42, with_parens: true}
///  |  |   |     ^ Index (0)
///  |  |   ^ Str (baz)
///  |  ^ Id (bar)
///  ^ Root (foo)
#[derive(Clone, Debug, PartialEq)]
pub enum LookupNode {
    /// The root of the lookup chain
    Root(AstIndex),
    /// A `.` access using an identifier
    Id(ConstantIndex),
    /// A `.` access using a string
    Str(AstString),
    /// An index operation using square `[]` brackets.
    Index(AstIndex),
    /// A function call
    Call {
        /// The arguments used in the function call
        args: Vec<AstIndex>,
        /// Whether or not parentheses are present in the function call
        ///
        /// This is not cosmetic, as parentheses represent a 'closed call', which has an impact on
        /// function piping:
        /// e.g.
        ///   `99 >> foo.bar 42` is equivalent to `foo.bar(42, 99)`
        /// but:
        ///   `99 >> foo.bar(42)` is equivalent to `foo.bar(42)(99)`.
        with_parens: bool,
    },
}

/// An assignment target with its associated scope
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AssignTarget {
    /// The target of the assignment
    pub target_index: AstIndex,
    /// The scope of the assignment
    pub scope: Scope,
}

/// An arm in a match expression
#[derive(Clone, Debug, PartialEq)]
pub struct MatchArm {
    /// A series of match patterns
    pub patterns: Vec<AstIndex>,
    /// An optional condition for the match arm
    ///
    /// e.g.
    /// match foo
    ///   bar if check_condition bar then ...
    pub condition: Option<AstIndex>,
    /// The body of the match arm
    pub expression: AstIndex,
}

/// An arm in a switch expression
#[derive(Clone, Debug, PartialEq)]
pub struct SwitchArm {
    /// An optional condition for the switch arm
    ///
    /// None implies `else`, and should always appear as the last arm.
    pub condition: Option<AstIndex>,
    /// The body of the switch arm
    pub expression: AstIndex,
}

/// A meta key
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
pub enum MetaKeyId {
    /// @+
    Add,
    /// @-
    Subtract,
    /// @*
    Multiply,
    /// @/
    Divide,
    /// @%
    Modulo,
    /// @<
    Less,
    /// @<=
    LessOrEqual,
    /// @>
    Greater,
    /// @>=
    GreaterOrEqual,
    /// @==
    Equal,
    /// @!=
    NotEqual,
    /// @[]
    Index,

    /// @display
    Display,
    /// @iterator
    Iterator,
    /// @negate
    Negate,
    /// @not
    Not,
    /// @type
    Type,

    /// @tests
    Tests,
    /// @test test_name
    Test,
    /// @pre_test
    PreTest,
    /// @post_test
    PostTest,

    /// @main
    Main,

    /// @meta name
    Named,

    /// Unused
    ///
    /// This entry must be last, see TryFrom<u7> for [MetaKeyId]
    Invalid,
}

impl TryFrom<u8> for MetaKeyId {
    type Error = u8;

    fn try_from(byte: u8) -> Result<Self, Self::Error> {
        if byte < Self::Invalid as u8 {
            // Safety: Any value less than Invalid is safe to transmute.
            Ok(unsafe { std::mem::transmute(byte) })
        } else {
            Err(byte)
        }
    }
}

/// A map key definition
#[derive(Clone, Debug, PartialEq)]
pub enum MapKey {
    /// An identifier
    Id(ConstantIndex),
    /// A string
    Str(AstString),
    /// A meta key
    ///
    /// Some meta keys require an additional identifier, e.g. @test test_name
    Meta(MetaKeyId, Option<ConstantIndex>),
}

/// The type of quotation mark used in a string literal
#[derive(Clone, Copy, Debug, PartialEq)]
#[allow(missing_docs)]
pub enum QuotationMark {
    Double,
    Single,
}

/// A node in an import item, see [Node::Import]
#[derive(Clone, Debug, PartialEq)]
pub enum ImportItemNode {
    /// An identifier node
    ///
    /// e.g. import foo.bar
    ///                 ^ Id(bar)
    ///             ^ Id(foo)
    Id(ConstantIndex),
    /// A string node
    ///
    /// e.g. import "foo/bar"
    ///             ^ Str("foo/bar")
    Str(AstString),
}
