use crate::common;
use crate::expr;
use crate::lexer::TokenKind;
use crate::Parser;
use smql_ast::query::*;
use smql_ast::types::{AggregateFunction, SortClause, SortDirection};
use smql_ast::view::{ProjectionDefinition, RefreshPolicy, ViewDefinition};
use smql_ast::{SmqlError, SmqlResult};

/// Parse GET Machine instance_id [AS ACTOR role] | GET VIEW name | GET PROJECTION name.
pub fn parse_get(parser: &mut Parser) -> SmqlResult<smql_ast::query::Query> {
    parser.expect_keyword("GET")?;
    // Peek ahead: GET VIEW name | GET PROJECTION name | GET Machine id
    if parser.try_keyword("VIEW") {
        let name = parser.expect_ident()?;
        return Ok(Query::GetView(GetViewQuery { name }));
    }
    if parser.try_keyword("PROJECTION") {
        let name = parser.expect_ident()?;
        return Ok(Query::GetProjection(GetProjectionQuery { name }));
    }
    let machine = parser.expect_ident()?;
    let instance_id = parser.expect_ident_or_string()?;
    let as_actor = if parser.try_keyword("AS") {
        parser.expect_keyword("ACTOR")?;
        Some(parser.expect_ident()?)
    } else {
        None
    };
    Ok(Query::Get(GetQuery {
        machine,
        instance_id,
        as_actor,
    }))
}

/// Parse FIND Machine [WHERE ...] [SORT ...] [LIMIT n] [OFFSET n].
pub fn parse_find(parser: &mut Parser) -> SmqlResult<smql_ast::query::Query> {
    parser.expect_keyword("FIND")?;
    let machine = parser.expect_ident()?;

    let filter = if parser.try_keyword("WHERE") {
        Some(expr::parse_expression(parser)?)
    } else {
        None
    };

    let mut sort = Vec::new();
    if parser.try_keyword("SORT") {
        parser.try_keyword("BY");
        loop {
            let field = parser.expect_ident()?;
            let direction = if parser.try_keyword("DESC") {
                SortDirection::Desc
            } else {
                parser.try_keyword("ASC");
                SortDirection::Asc
            };
            sort.push(SortClause { field, direction });
            if !parser.try_punct(",") {
                break;
            }
        }
    }

    let limit = if parser.try_keyword("LIMIT") {
        Some(common::parse_non_negative_int(parser)?)
    } else {
        None
    };

    let offset = if parser.try_keyword("OFFSET") {
        Some(common::parse_non_negative_int(parser)?)
    } else {
        None
    };

    let after = if parser.try_keyword("AFTER") {
        Some(parser.expect_ident_or_string()?)
    } else {
        None
    };

    let as_actor = if parser.try_keyword("AS") {
        parser.expect_keyword("ACTOR")?;
        Some(parser.expect_ident()?)
    } else {
        None
    };

    Ok(Query::Find(FindQuery {
        machine,
        filter,
        sort,
        limit,
        offset,
        after,
        as_actor,
    }))
}

/// Parse AGGREGATE Machine MEASURE ... [WHERE ...] [GROUP BY ...].
pub fn parse_aggregate(parser: &mut Parser) -> SmqlResult<smql_ast::query::Query> {
    parser.expect_keyword("AGGREGATE")?;
    let machine = parser.expect_ident()?;

    let mut measures = Vec::new();
    let mut filter = None;
    let mut group_by = Vec::new();

    while !parser.is_eof() {
        if parser.try_keyword("MEASURE") {
            loop {
                let measure = parse_measure_clause(parser)?;
                measures.push(measure);
                if !parser.try_punct(",") {
                    break;
                }
            }
        } else if parser.try_keyword("WHERE") {
            filter = Some(expr::parse_expression(parser)?);
        } else if parser.try_keyword("GROUP") {
            parser.expect_keyword("BY")?;
            loop {
                if parser.check_keyword("STATE") {
                    parser.advance()?;
                    group_by.push(GroupByClause::State);
                } else {
                    let field = parser.expect_ident()?;
                    group_by.push(GroupByClause::Field(field));
                }
                if !parser.try_punct(",") {
                    break;
                }
            }
        } else {
            break;
        }
    }

    Ok(Query::Aggregate(AggregateQuery {
        machine,
        measures,
        filter,
        group_by,
    }))
}

fn parse_measure_clause(parser: &mut Parser) -> SmqlResult<MeasureClause> {
    let func = parse_aggregate_function(parser)?;
    parser.expect_punct("(")?;
    let field = if parser.check_punct(")") {
        None
    } else {
        Some(parser.expect_ident()?)
    };
    parser.expect_punct(")")?;

    let alias = if parser.try_keyword("AS") {
        Some(parser.expect_ident()?)
    } else {
        None
    };

    Ok(MeasureClause {
        function: func,
        field,
        alias,
    })
}

fn parse_aggregate_function(parser: &mut Parser) -> SmqlResult<AggregateFunction> {
    let tok = parser.advance()?;
    match &tok.kind {
        TokenKind::Keyword(k) => {
            match k.as_str() {
                "COUNT" => Ok(AggregateFunction::Count),
                "SUM" => Ok(AggregateFunction::Sum),
                "AVG" => Ok(AggregateFunction::Avg),
                "MIN" => Ok(AggregateFunction::Min),
                "MAX" => Ok(AggregateFunction::Max),
                "PERCENTILE" => Ok(AggregateFunction::Percentile(50.0)), // Default to median (p50)
                _ => Err(SmqlError::ParseError {
                    message: format!("Unknown aggregate function '{}'", k),
                    span: None,
                    hint: Some("Valid functions: COUNT, SUM, AVG, MIN, MAX, PERCENTILE".into()),
                }),
            }
        }
        _ => Err(SmqlError::parse("Expected aggregate function name")),
    }
}

/// Parse TRAIL OF instance_id [WHERE ...].
pub fn parse_trail(parser: &mut Parser) -> SmqlResult<smql_ast::query::Query> {
    parser.expect_keyword("TRAIL")?;
    parser.expect_keyword("OF")?;
    let instance_id = parser.expect_ident_or_string()?;

    let filter = if parser.try_keyword("WHERE") {
        let mut trail_filter = TrailFilter {
            actor: None,
            from_state: None,
            to_state: None,
            since: None,
            until: None,
        };
        // Parse trail-specific filter clauses: ACTOR, FROM, TO
        loop {
            if parser.try_keyword("ACTOR") {
                trail_filter.actor = Some(parser.expect_ident_or_string()?);
            } else if parser.try_keyword("FROM") {
                trail_filter.from_state = Some(parser.expect_ident_or_string()?);
            } else if parser.try_keyword("TO") {
                trail_filter.to_state = Some(parser.expect_ident_or_string()?);
            } else {
                break;
            }
            // Optional comma between clauses
            parser.try_punct(",");
        }
        Some(trail_filter)
    } else {
        None
    };

    Ok(Query::Trail(TrailQuery {
        machine: None,
        instance_id,
        filter,
    }))
}

/// Parse PATHS FROM Machine [WHERE ...] [LIMIT n].
pub fn parse_paths(parser: &mut Parser) -> SmqlResult<smql_ast::query::Query> {
    parser.expect_keyword("PATHS")?;
    parser.expect_keyword("FROM")?;
    let machine = parser.expect_ident()?;

    let filter = if parser.try_keyword("WHERE") {
        Some(expr::parse_expression(parser)?)
    } else {
        None
    };

    let limit = if parser.try_keyword("LIMIT") {
        Some(common::parse_non_negative_int(parser)?)
    } else {
        None
    };

    Ok(Query::Paths(PathsQuery {
        machine,
        filter,
        limit,
    }))
}

/// Parse FUNNEL Machine THROUGH [state1, state2, ...] [WHERE ...].
pub fn parse_funnel(parser: &mut Parser) -> SmqlResult<smql_ast::query::Query> {
    parser.expect_keyword("FUNNEL")?;
    let machine = parser.expect_ident()?;
    parser.expect_keyword("THROUGH")?;
    parser.expect_punct("[")?;
    let mut states = Vec::new();
    while !parser.check_punct("]") {
        states.push(parser.expect_ident()?);
        parser.try_punct(",");
    }
    parser.expect_punct("]")?;

    let filter = if parser.try_keyword("WHERE") {
        Some(expr::parse_expression(parser)?)
    } else {
        None
    };

    Ok(Query::Funnel(FunnelQuery {
        machine,
        states,
        filter,
    }))
}

/// Parse DEFINE VIEW Name AS FIND ...
pub fn parse_define_view(parser: &mut Parser) -> SmqlResult<ViewDefinition> {
    parser.expect_keyword("DEFINE")?;
    parser.expect_keyword("VIEW")?;
    let name = parser.expect_ident()?;
    parser.expect_keyword("AS")?;
    // Parse the underlying FIND query (without the Query wrapper)
    parser.expect_keyword("FIND")?;
    let machine = parser.expect_ident()?;

    let filter = if parser.try_keyword("WHERE") {
        Some(expr::parse_expression(parser)?)
    } else {
        None
    };

    let mut sort = Vec::new();
    if parser.try_keyword("SORT") {
        parser.try_keyword("BY");
        loop {
            let field = parser.expect_ident()?;
            let direction = if parser.try_keyword("DESC") {
                SortDirection::Desc
            } else {
                parser.try_keyword("ASC");
                SortDirection::Asc
            };
            sort.push(SortClause { field, direction });
            if !parser.try_punct(",") {
                break;
            }
        }
    }

    let limit = if parser.try_keyword("LIMIT") {
        Some(common::parse_non_negative_int(parser)?)
    } else {
        None
    };

    let offset = if parser.try_keyword("OFFSET") {
        Some(common::parse_non_negative_int(parser)?)
    } else {
        None
    };

    Ok(ViewDefinition {
        name,
        query: FindQuery {
            machine,
            filter,
            sort,
            limit,
            offset,
            after: None,
            as_actor: None,
        },
    })
}

/// Parse DEFINE PROJECTION Name AS AGGREGATE ... REFRESH ON TRANSITION | ON INTERVAL n | MANUAL
pub fn parse_define_projection(parser: &mut Parser) -> SmqlResult<ProjectionDefinition> {
    parser.expect_keyword("DEFINE")?;
    parser.expect_keyword("PROJECTION")?;
    let name = parser.expect_ident()?;
    parser.expect_keyword("AS")?;
    parser.expect_keyword("AGGREGATE")?;
    let machine = parser.expect_ident()?;

    let mut measures = Vec::new();
    let mut filter = None;
    let mut group_by = Vec::new();

    // Parse AGGREGATE body until REFRESH keyword
    while !parser.is_eof() && !parser.check_keyword("REFRESH") {
        if parser.try_keyword("MEASURE") {
            loop {
                let measure = parse_measure_clause(parser)?;
                measures.push(measure);
                if !parser.try_punct(",") {
                    break;
                }
            }
        } else if parser.try_keyword("WHERE") {
            filter = Some(expr::parse_expression(parser)?);
        } else if parser.try_keyword("GROUP") {
            parser.expect_keyword("BY")?;
            loop {
                if parser.check_keyword("STATE") {
                    parser.advance()?;
                    group_by.push(GroupByClause::State);
                } else {
                    let field = parser.expect_ident()?;
                    group_by.push(GroupByClause::Field(field));
                }
                if !parser.try_punct(",") {
                    break;
                }
            }
        } else {
            break;
        }
    }

    // Parse REFRESH policy
    let refresh = if parser.try_keyword("REFRESH") {
        parser.expect_keyword("ON")?;
        if parser.try_keyword("TRANSITION") {
            RefreshPolicy::OnTransition
        } else if parser.try_keyword("INTERVAL") {
            let secs = common::parse_non_negative_int(parser)?;
            RefreshPolicy::OnInterval(secs)
        } else {
            RefreshPolicy::Manual
        }
    } else {
        RefreshPolicy::Manual
    };

    Ok(ProjectionDefinition {
        name,
        query: AggregateQuery {
            machine,
            measures,
            filter,
            group_by,
        },
        refresh,
    })
}

/// Parse COMPARE PATHS Machine SEGMENT BY field [WHERE ...].
pub fn parse_compare_paths(parser: &mut Parser) -> SmqlResult<smql_ast::query::Query> {
    parser.expect_keyword("COMPARE")?;
    parser.expect_keyword("PATHS")?;
    let machine = parser.expect_ident()?;
    parser.expect_keyword("SEGMENT")?;
    parser.expect_keyword("BY")?;
    let segment_by = parser.expect_ident()?;

    let filter = if parser.try_keyword("WHERE") {
        Some(expr::parse_expression(parser)?)
    } else {
        None
    };

    Ok(Query::ComparePaths(ComparePathsQuery {
        machine,
        segment_by,
        filter,
    }))
}
