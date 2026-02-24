export enum SmqlErrorCode {
  BadRequest = "BAD_REQUEST",
  NotFound = "NOT_FOUND",
  TransitionDenied = "TRANSITION_DENIED",
  Unauthorized = "UNAUTHORIZED",
  Conflict = "CONFLICT",
  ServerError = "SERVER_ERROR",
  Network = "NETWORK",
  Timeout = "TIMEOUT",
  Subscription = "SUBSCRIPTION",
}

export class SmqlError extends Error {
  readonly code: SmqlErrorCode;
  readonly statusCode?: number;

  constructor(message: string, code: SmqlErrorCode, statusCode?: number) {
    super(message);
    this.name = "SmqlError";
    this.code = code;
    this.statusCode = statusCode;
  }
}

export class BadRequestError extends SmqlError {
  constructor(message: string) {
    super(message, SmqlErrorCode.BadRequest, 400);
    this.name = "BadRequestError";
  }
}

export class UnauthorizedError extends SmqlError {
  constructor(message: string) {
    super(message, SmqlErrorCode.Unauthorized, 401);
    this.name = "UnauthorizedError";
  }
}

export class NotFoundError extends SmqlError {
  constructor(message: string) {
    super(message, SmqlErrorCode.NotFound, 404);
    this.name = "NotFoundError";
  }
}

export class TransitionDeniedError extends SmqlError {
  constructor(message: string) {
    super(message, SmqlErrorCode.TransitionDenied, 409);
    this.name = "TransitionDeniedError";
  }
}

export class ConflictError extends SmqlError {
  constructor(message: string) {
    super(message, SmqlErrorCode.Conflict, 409);
    this.name = "ConflictError";
  }
}

export class NetworkError extends SmqlError {
  constructor(message: string) {
    super(message, SmqlErrorCode.Network);
    this.name = "NetworkError";
  }
}

export class TimeoutError extends SmqlError {
  constructor(message: string) {
    super(message, SmqlErrorCode.Timeout);
    this.name = "TimeoutError";
  }
}

export class SubscriptionError extends SmqlError {
  constructor(message: string) {
    super(message, SmqlErrorCode.Subscription);
    this.name = "SubscriptionError";
  }
}
