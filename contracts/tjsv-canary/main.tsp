namespace Fiducia.Test;

model LeaseProbe {
  @minLength(1)
  leaseId: string;

  @minLength(1)
  holder: string;

  active: boolean;
}
