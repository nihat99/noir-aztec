use crate::{ArraySize, Type, hir::lower::{HirBinaryOp, HirExpression, HirLiteral, def_interner::{DefInterner, ExprId, IdentId, StmtId}, function::Param, stmt::HirStatement}};

pub(crate) fn type_check_expression(interner : &mut DefInterner, expr_id : ExprId) {
    let hir_expr = interner.expression(expr_id);
    match hir_expr {
        HirExpression::Ident(ident_id) => {
            // If an Ident is used in an expression, it cannot be a declaration statement  
            let ident_def_id = interner.ident_def(&ident_id).expect("ice: all identifiers should have been resolved. this should have been caught in the resolver");

            // The type of this Ident expression is the type of the Identifier which defined it
            let typ = interner.id_type(ident_def_id.into());
            interner.push_expr_type(expr_id, typ.clone());
        }
        HirExpression::Literal(literal) => {
            match literal {
                HirLiteral::Array(arr) => {
                    let mut arr_types = Vec::with_capacity(arr.contents.len());
                    for element_expr_id in arr.contents {
                        // Type check the contents of the array
                        type_check_expression(interner, element_expr_id);
                        arr_types.push(interner.id_type(element_expr_id.into())) 
                    }
                    
                    // Specify the type of the Array
                    // Note: This assumes that the array is homogenous, which will be checked next
                    let arr_type = Type::Array(ArraySize::Fixed(arr_types.len() as u128), Box::new(arr_types[0].clone()));
                
                    // Check if the array is homogenous
                    //
                    // An array with one element will be homogenous
                    if arr_types.len() == 1{
                        interner.push_expr_type(expr_id, arr_type);
                        return 
                    }

                    // To check if an array with more than one element
                    // is homogenous, we can use a sliding window of size two 
                    // to check if adjacent elements are the same
                    // Note: windows(2) expects there to be two or more values
                    // So the case of one element is an edge case which would panic in the compiler.
                    //
                    // XXX: We can refactor this algorithm to peek ahead and check instead of using window.
                    // It would allow us to not need to check the case of one, but it's not significant. 
                    for (_,type_pair) in arr_types.windows(2).enumerate() {
                        let left_type = &type_pair[0]; 
                        let right_type = &type_pair[1]; 

                        if left_type != right_type {
                            panic!("type {} does not equal type {} in the array", left_type, right_type)
                        }
                    }

                    interner.push_expr_type(expr_id, arr_type)
                }
                HirLiteral::Bool(_) => {
                    unimplemented!("currently native boolean types have not been implemented")
                }
                HirLiteral::Integer(_) => {
                    // Literal integers will always be a constant, since the lexer was able to parse the integer
                    interner.push_expr_type(expr_id, Type::Constant);
                }
                HirLiteral::Str(_) => unimplemented!("[Coming Soon] : Currently string literal types have not been implemented"),

            }
        }

        HirExpression::Infix(infix_expr) => {
            // The type of the infix expression must be looked up from a type table
            
            type_check_expression(interner, infix_expr.lhs);
            let lhs_type = interner.id_type(infix_expr.lhs.into());
            
            type_check_expression(interner, infix_expr.rhs);
            let rhs_type = interner.id_type(infix_expr.rhs.into());

            let result_type = infix_operand_type_rules(&lhs_type,&infix_expr.operator, &rhs_type).expect("error reporting has been rolled back. Type mismatch");
            interner.push_expr_type(expr_id, result_type);
        }
        HirExpression::Index(index_expr) => {
            let ident_def = interner.ident_def(&index_expr.collection_name).expect("ice : all identifiers should have a def");
            let collection_type = interner.id_type(ident_def.into());
            match collection_type {
                // XXX: We can check the array bounds here also, but it may be better to constant fold first
                // and have ConstId instead of ExprId for constants
                Type::Array(_, base_type) => {interner.push_expr_type(expr_id, *base_type)},
                _=> panic!("error reporting has been rolled back. Type is not an array")
            };

        }
        HirExpression::Call(call_expr) => {
            let func_meta = interner.function_meta(call_expr.func_id);

            // Check function call arity is correct
            let param_len = func_meta.parameters.len();
            let arg_len = call_expr.arguments.len();
            if param_len != arg_len {
                panic!("error reporting has been reverted. expected {} number of arguments, got {} number of arguments", param_len, arg_len)
            }

            // Type check arguments
            let mut arg_types = Vec::with_capacity(call_expr.arguments.len());
            for arg_expr in call_expr.arguments {
                type_check_expression(interner, arg_expr);
                arg_types.push(interner.id_type(arg_expr.into())) 
            }

            // Check for argument param equality
            for (param, arg) in func_meta.parameters.iter().zip(arg_types) {
                check_param_argument(param, &arg)
            }

            // The type of the call expression is the return type of the function being called
            interner.push_expr_type(expr_id, func_meta.return_type);
        }
        HirExpression::Cast(cast_expr) => {
            // Evaluate the Lhs
            type_check_expression(interner, cast_expr.lhs);
            let _lhs_type = interner.id_type(cast_expr.lhs.into());

            // Then check that the type_of(LHS) can be casted to the RHS
            // This is currently being done in the evaluator, we should move it all to here
            // XXX(^) : Move checks for casting from runtime to here

            // type_of(cast_expr) == type_of(cast_type)
            interner.push_expr_type(expr_id, cast_expr.r#type);
        }
        HirExpression::For(for_expr) => {
            type_check_expression(interner, for_expr.start_range);
            type_check_expression(interner, for_expr.end_range);

            let start_range_type = interner.id_type(for_expr.start_range.into());
            let end_range_type = interner.id_type(for_expr.end_range.into());

            if start_range_type != Type::Constant {
                panic!("error reporting has been reverted. start range is not a constant");
            }
            if end_range_type != Type::Constant {
                panic!("error reporting has been reverted. end range is not a constant");
            }
            
            // This check is only needed, if we decide to not have constant range bounds.
            if start_range_type != end_range_type {
                panic!("error reporting has been reverted. start range and end range have different types");
            }
            // The type of the identifier is equal to the type of the ranges
            interner.push_ident_type(for_expr.identifier, start_range_type);

            super::stmt::type_check(interner, for_expr.block);

            let last_type = extract_last_type_from_block(interner,for_expr.block);

            // XXX: In the release before this, we were using the start and end range to determine the number
            // of iterations and marking the type as Fixed. Is this still necessary?
            // It may be possible to do this properly again, once we do constant folding. Since the range will always be const expr 
            interner.push_expr_type(expr_id, Type::Array(ArraySize::Variable, Box::new(last_type)));
        },
        HirExpression::Prefix(_) => {
            // type_of(prefix_expr) == type_of(rhs_expression)
            todo!("prefix expressions have not been implemented yet")
        },
        HirExpression::Predicate(_) => {todo!("predicate statements have not been implemented yet")},
        HirExpression::If(_) => todo!("If statements have not been implemented yet!")
    }
}

    // Given a binary operator and another type. This method will produce the 
    // output type
    pub fn infix_operand_type_rules(lhs_type : &Type, op : &HirBinaryOp, other: &Type) -> Result<Type, String> {
        if op.is_comparator() {
            return Ok(Type::Bool)
        }
        
        match (lhs_type, other)  {

            (Type::Integer(sign_x, bit_width_x), Type::Integer(sign_y, bit_width_y)) => {
                if sign_x != sign_y {
                    return Err(format!("Integers must have the same Signedness lhs is {:?}, rhs is {:?} ", sign_x, sign_y))
                }
                if bit_width_x != bit_width_y {
                    return Err(format!("Integers must have the same Bit width lhs is {}, rhs is {} ", bit_width_x, bit_width_y))
                }
                Ok(Type::Integer(*sign_x, *bit_width_x))
            }
            (Type::Integer(_, _), Type::Witness) | ( Type::Witness, Type::Integer(_, _) ) => { 
                Err(format!("Cannot use an integer and a witness in a binary operation, try converting the witness into an integer"))
            }
            (Type::Integer(sign_x, bit_width_x), Type::Constant)| (Type::Constant,Type::Integer(sign_x, bit_width_x)) => {
                Ok(Type::Integer(*sign_x, *bit_width_x))
            }
            (Type::Integer(_, _), typ) | (typ,Type::Integer(_, _)) => {
                Err(format!("Integer cannot be used with type {:?}", typ))
            }

            // Currently, arrays are not supported in binary operations
            (Type::Array(_,_), _) | (_,Type::Array(_, _)) => Err(format!("Arrays cannot be used in an infix operation")),
            
            // An error type on either side will always return an error
            (Type::Error, _) | (_,Type::Error) => Ok(Type::Error),
            (Type::Unspecified, _) | (_,Type::Unspecified) => Ok(Type::Unspecified),
            (Type::Unknown, _) | (_,Type::Unknown) => Ok(Type::Unknown),
            (Type::Unit, _) | (_,Type::Unit) => Ok(Type::Unit),

            // If no side contains an integer. Then we check if either side contains a witness
            // If either side contains a witness, then the final result will be a witness
            (Type::Witness, _) | (_,Type::Witness) => Ok(Type::Witness),
            // Public types are added as witnesses under the hood
            (Type::Public, _) | (_,Type::Public) => Ok(Type::Witness),
            (Type::Bool, _) | (_,Type::Bool) => Ok(Type::Bool),

            (Type::FieldElement, _) | (_,Type::FieldElement) => Ok(Type::FieldElement),
            
            (Type::Constant, Type::Constant)  => Ok(Type::Constant),
        }
        
    }

fn check_param_argument(param : &Param, arg_type : &Type) {

        let param_type = &param.1;
        let param_id = param.0;

        if arg_type.is_variable_sized_array() {
            panic!("arg_type type cannot be a variable sized array")
        }
        
        // Variable sized arrays (vectors) can be linked to fixed size arrays
        // If the parameter specifies a variable sized array, then we can pass a 
        // fixed size array as an argument
        if param_type.is_variable_sized_array() && arg_type.is_fixed_sized_array() {
            return
        }
        
        if param_type != arg_type {
            panic!("Expected {} for parameter {:?} but got {} ", param_type,param_id, arg_type)
        }        
}

// XXX: Currently, we do not have BlockExpressions, so we need to extract the last expression from 
// a block statement until then 
// This will be removed once BlockExpressions are added.
fn extract_last_type_from_block(interner : &DefInterner, stmt_id : StmtId) -> Type {
    let stmt = interner.statement(stmt_id);
    match stmt {
            HirStatement::Block(block_stmt) => {
                let statements =  block_stmt.statements();
                if statements.len() == 0 {
                    return Type::Unit
                }
                let last_stmt_id = statements.last().unwrap();
                
                let last_stmt = interner.statement(*last_stmt_id);
                // If the last statement is an expression statement, then we take the value
                // if not, then we return Unit
                match last_stmt {
                    HirStatement::Expression(expr_id) => return interner.id_type(expr_id.into()),
                    HirStatement::Block(_) => panic!("{}","this should not be possible to do right now, as {/*code*/} is not supported"),
                    _=> return Type::Unit
                }
            },
            _=> panic!("This statement should have been a block stmt")
        }
}