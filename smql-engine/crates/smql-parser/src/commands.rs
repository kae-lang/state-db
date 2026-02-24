use crate::common;
use crate::expr;
use crate::Parser;
use smql_ast::command::*;
use smql_ast::machine::TransitionDefinition;
use smql_ast::{SmqlError, SmqlResult};

/// Parse SPAWN Machine { field: value, ... } [THEN TRANSITION TO state].
pub fn parse_spawn(parser: &mut Parser) -> SmqlResult<Command> {
    parser.expect_keyword("SPAWN")?;

    // Check for SPAWN BATCH
    let batch = parser.try_keyword("BATCH");

    let machine_name = parser.expect_ident()?;

    if batch {
        // SPAWN BATCH Machine [ { ... }, { ... }, ... ]
        parser.expect_punct("[")?;
        let mut batch_data = Vec::new();
        while !parser.check_punct("]") {
            let data = parse_spawn_data(parser)?;
            batch_data.push(data);
            parser.try_punct(",");
        }
        parser.expect_punct("]")?;
        return Ok(Command::Spawn(SpawnCommand {
            machine: machine_name,
            data: Vec::new(),
            then_transition: None,
            batch: true,
            batch_data,
            parent_id: None,
            parent_machine: None,
            as_actor: None,
            idempotency_key: None,
            tags: Vec::new(),
            ttl: None,
        }));
    }

    let data = parse_spawn_data(parser)?;

    let then_transition = if parser.try_keyword("THEN") {
        parser.expect_keyword("TRANSITION")?;
        parser.expect_keyword("TO")?;
        Some(parser.expect_ident()?)
    } else {
        None
    };

    let idempotency_key = if parser.try_keyword("IDEMPOTENCY_KEY") {
        Some(common::parse_string_literal(parser)?)
    } else {
        None
    };

    let tags = if parser.try_keyword("TAGS") {
        parse_tags_block(parser)?
    } else {
        Vec::new()
    };

    let ttl = if parser.try_keyword("TTL") {
        Some(common::parse_duration(parser)?)
    } else {
        None
    };

    Ok(Command::Spawn(SpawnCommand {
        machine: machine_name,
        data,
        then_transition,
        batch: false,
        batch_data: Vec::new(),
        parent_id: None,
        parent_machine: None,
        as_actor: None,
        idempotency_key,
        tags,
        ttl,
    }))
}

fn parse_spawn_data(
    parser: &mut Parser,
) -> SmqlResult<Vec<(String, smql_ast::expression::Expression)>> {
    parser.expect_punct("{")?;
    let mut data = Vec::new();
    while !parser.check_punct("}") {
        let field = parser.expect_ident()?;
        parser.expect_punct(":")?;
        let value = expr::parse_expression(parser)?;
        data.push((field, value));
        parser.try_punct(",");
    }
    parser.expect_punct("}")?;
    Ok(data)
}

/// Parse TRANSITION instance_id TO state [WITH { ... }] [MEMO "..."] [AS actor].
pub fn parse_transition(parser: &mut Parser) -> SmqlResult<Command> {
    parser.expect_keyword("TRANSITION")?;

    // Check for TRANSITION ALL Machine WHERE ... TO state
    if parser.check_keyword("ALL") {
        return parse_batch_transition(parser);
    }

    let machine_name = parser.expect_ident()?;
    let instance_id = parser.expect_ident_or_string()?;
    parser.expect_keyword("TO")?;
    let to_state = parser.expect_ident()?;

    let mut cmd = TransitionCommand::new(machine_name, instance_id, to_state);

    // Parse optional clauses
    loop {
        if parser.try_keyword("WITH") {
            cmd.with_data = parse_spawn_data(parser)?;
        } else if parser.try_keyword("MEMO") {
            cmd.memo = Some(common::parse_string_literal(parser)?);
        } else if parser.try_keyword("AS") {
            cmd.as_actor = Some(parser.expect_ident_or_string()?);
        } else if parser.try_keyword("THROUGH") {
            parser.expect_punct("[")?;
            while !parser.check_punct("]") {
                cmd.through.push(parser.expect_ident()?);
                parser.try_punct(",");
            }
            parser.expect_punct("]")?;
        } else if parser.try_keyword("OR_STAY") {
            cmd.or_stay = true;
        } else if parser.try_keyword("CASCADE") {
            cmd.cascade = true;
        } else if parser.try_keyword("IDEMPOTENCY_KEY") {
            cmd.idempotency_key = Some(common::parse_string_literal(parser)?);
        } else if parser.try_keyword("TAGS") {
            cmd.tags = parse_tags_block(parser)?;
        } else {
            break;
        }
    }

    Ok(Command::Transition(cmd))
}

/// Parse TRY TRANSITION instance_id TO state ...
pub fn parse_try_transition(parser: &mut Parser) -> SmqlResult<Command> {
    parser.expect_keyword("TRY")?;
    parser.expect_keyword("TRANSITION")?;

    let machine_name = parser.expect_ident()?;
    let instance_id = parser.expect_ident_or_string()?;
    parser.expect_keyword("TO")?;
    let to_state = parser.expect_ident()?;

    let mut cmd = TransitionCommand::new(machine_name, instance_id, to_state);

    loop {
        if parser.try_keyword("WITH") {
            cmd.with_data = parse_spawn_data(parser)?;
        } else if parser.try_keyword("MEMO") {
            cmd.memo = Some(common::parse_string_literal(parser)?);
        } else if parser.try_keyword("AS") {
            cmd.as_actor = Some(parser.expect_ident_or_string()?);
        } else if parser.try_keyword("IDEMPOTENCY_KEY") {
            cmd.idempotency_key = Some(common::parse_string_literal(parser)?);
        } else if parser.try_keyword("TAGS") {
            cmd.tags = parse_tags_block(parser)?;
        } else {
            break;
        }
    }

    Ok(Command::TryTransition(cmd))
}

fn parse_batch_transition(parser: &mut Parser) -> SmqlResult<Command> {
    parser.expect_keyword("ALL")?;
    let machine = parser.expect_ident()?;
    parser.expect_keyword("WHERE")?;
    let filter = expr::parse_expression(parser)?;
    parser.expect_keyword("TO")?;
    let to_state = parser.expect_ident()?;

    let mut cmd = BatchTransitionCommand {
        machine,
        filter,
        to_state,
        with_data: Vec::new(),
        memo: None,
        as_actor: None,
    };

    loop {
        if parser.try_keyword("WITH") {
            cmd.with_data = parse_spawn_data(parser)?;
        } else if parser.try_keyword("MEMO") {
            cmd.memo = Some(common::parse_string_literal(parser)?);
        } else if parser.try_keyword("AS") {
            cmd.as_actor = Some(parser.expect_ident()?);
        } else {
            break;
        }
    }

    Ok(Command::BatchTransition(cmd))
}

/// Parse ALTER MACHINE Name operations.
pub fn parse_alter_machine(parser: &mut Parser) -> SmqlResult<Command> {
    parser.expect_keyword("ALTER")?;
    parser.expect_keyword("MACHINE")?;
    let machine = parser.expect_ident()?;

    let mut operations = Vec::new();

    while !parser.is_eof() {
        if parser.try_keyword("ADD") {
            if parser.try_keyword("STATE") {
                let state = parser.expect_ident()?;
                operations.push(AlterOperation::AddState(state));
            } else if parser.try_keyword("TRANSITION") {
                // We'd need to parse a full transition definition here
                // For now, parse minimal: from -> to { ... }
                let from = parser.expect_ident()?;
                parser.expect_operator("->")?;
                let to = parser.expect_ident()?;
                let transition =
                    TransitionDefinition::new(smql_ast::machine::TransitionSource::State(from), to);
                operations.push(AlterOperation::AddTransition(transition));
            } else if parser.try_keyword("DATA") {
                let field = crate::machine::parse_data_field_from_alter(parser)?;
                let backfill = if parser.try_keyword("BACKFILL") {
                    Some(expr::parse_expression(parser)?)
                } else {
                    None
                };
                operations.push(AlterOperation::AddData { field, backfill });
            } else {
                return Err(SmqlError::parse(
                    "Expected STATE, TRANSITION, or DATA after ADD",
                ));
            }
        } else if parser.try_keyword("REMOVE") {
            if parser.try_keyword("STATE") {
                let state = parser.expect_ident()?;
                parser.expect_keyword("MIGRATE")?;
                parser.expect_keyword("TO")?;
                let migrate_to = parser.expect_ident()?;
                operations.push(AlterOperation::RemoveState { state, migrate_to });
            } else if parser.try_keyword("TRANSITION") {
                let from = parser.expect_ident()?;
                parser.expect_operator("->")?;
                let to = parser.expect_ident()?;
                operations.push(AlterOperation::RemoveTransition { from, to });
            } else if parser.try_keyword("DATA") {
                let field = parser.expect_ident()?;
                operations.push(AlterOperation::RemoveData(field));
            } else {
                return Err(SmqlError::parse(
                    "Expected STATE, TRANSITION, or DATA after REMOVE",
                ));
            }
        } else if parser.try_keyword("BACKFILL") {
            let field = parser.expect_ident()?;
            parser.expect_operator("=")?;
            let value = expr::parse_expression(parser)?;
            operations.push(AlterOperation::Backfill { field, value });
        } else {
            break;
        }
    }

    Ok(Command::AlterMachine(AlterMachineCommand {
        machine,
        operations,
    }))
}

/// Parse TAGS { key: "value", key2: "value2" }.
/// Parse CLAIM Machine WHERE ... AS agent LEASE duration [THEN TRANSITION TO state].
pub fn parse_claim(parser: &mut Parser) -> SmqlResult<Command> {
    parser.expect_keyword("CLAIM")?;
    let machine = parser.expect_ident()?;

    let filter = if parser.try_keyword("WHERE") {
        Some(expr::parse_expression(parser)?)
    } else {
        None
    };

    parser.expect_keyword("AS")?;
    let agent_id = common::parse_string_literal(parser)?;

    parser.expect_keyword("LEASE")?;
    let lease_duration = common::parse_duration(parser)?;

    let transition_to = if parser.try_keyword("THEN") {
        parser.expect_keyword("TRANSITION")?;
        parser.expect_keyword("TO")?;
        Some(parser.expect_ident()?)
    } else {
        None
    };

    Ok(Command::Claim(ClaimCommand {
        machine,
        filter,
        agent_id,
        lease_duration,
        transition_to,
    }))
}

/// Parse RELEASE Machine "instance_id" AS agent.
pub fn parse_release(parser: &mut Parser) -> SmqlResult<Command> {
    parser.expect_keyword("RELEASE")?;
    let machine = parser.expect_ident()?;
    let instance_id = common::parse_string_literal(parser)?;
    parser.expect_keyword("AS")?;
    let agent_id = common::parse_string_literal(parser)?;

    Ok(Command::Release(ReleaseCommand {
        machine,
        instance_id,
        agent_id,
    }))
}

/// Parse WATCH Machine "instance_id" UNTIL condition [TIMEOUT duration].
/// Also: WATCH Machine WHERE filter UNTIL condition [TIMEOUT duration].
pub fn parse_watch(parser: &mut Parser) -> SmqlResult<Command> {
    parser.expect_keyword("WATCH")?;
    let machine = parser.expect_ident()?;

    let (instance_id, filter) = if parser.try_keyword("WHERE") {
        // WATCH Machine WHERE ... UNTIL ...
        let filter_expr = expr::parse_expression(parser)?;
        (None, Some(filter_expr))
    } else {
        // WATCH Machine "instance_id" UNTIL ...
        let id = common::parse_string_literal(parser)?;
        (Some(id), None)
    };

    parser.expect_keyword("UNTIL")?;
    let condition = expr::parse_expression(parser)?;

    let timeout = if parser.try_keyword("TIMEOUT") {
        Some(common::parse_duration(parser)?)
    } else {
        None
    };

    Ok(Command::Watch(smql_ast::command::WatchCommand {
        machine,
        instance_id,
        condition,
        timeout,
        filter,
    }))
}

fn parse_tags_block(parser: &mut Parser) -> SmqlResult<Vec<(String, String)>> {
    parser.expect_punct("{")?;
    let mut tags = Vec::new();
    while !parser.check_punct("}") {
        let key = parser.expect_ident()?;
        parser.expect_punct(":")?;
        let value = common::parse_string_literal(parser)?;
        tags.push((key, value));
        parser.try_punct(",");
    }
    parser.expect_punct("}")?;
    Ok(tags)
}
