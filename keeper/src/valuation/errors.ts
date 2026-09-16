export class ValuationError extends Error {
  constructor(readonly reason: string) {
    super(reason);
    this.name = "ValuationError";
  }
}
