terraform {
  required_version = ">= 1.5"
}

variable "region" {
  type        = string
  description = "Where the collector runs."
  default     = "eu-west-1"
}

variable "azs" {
  type        = list(string)
  description = "Availability zones to spread the subnets across."
  default     = ["eu-west-1a", "eu-west-1b"]
}

variable "retention_days" {
  type        = number
  description = "How long a reading is kept."
  default     = 30
}

variable "instance_type" {
  type    = string
  default = "t3.small"
}

locals {
  name = "signals-collector"
  tags = {
    Service = local.name
    Region  = var.region
  }
}

resource "aws_vpc" "collector" {
  cidr_block           = "10.20.0.0/16"
  enable_dns_hostnames = true
  tags                 = local.tags
}

resource "aws_subnet" "collector" {
  count             = length(var.azs)
  vpc_id            = aws_vpc.collector.id
  availability_zone = var.azs[count.index]
  cidr_block        = cidrsubnet(aws_vpc.collector.cidr_block, 8, count.index)
  tags              = local.tags
}

resource "aws_instance" "collector" {
  instance_type = var.instance_type
  subnet_id     = aws_subnet.collector[0].id
  tags          = local.tags
}

output "vpc_id" {
  value = aws_vpc.collector.id
}

output "subnet_ids" {
  value = aws_subnet.collector[*].id
}

output "retention_days" {
  value = var.retention_days
}
