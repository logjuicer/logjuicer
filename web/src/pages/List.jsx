// Copyright 2018 Red Hat, Inc
//
// Licensed under the Apache License, Version 2.0 (the "License"); you may
// not use this file except in compliance with the License. You may obtain
// a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
// WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied. See the
// License for the specific language governing permissions and limitations
// under the License.

import React from 'react'
import PropTypes from 'prop-types'
import { connect } from 'react-redux'
import { Link } from 'react-router-dom'
import { Icon, OverlayTrigger, Table, Tooltip } from 'patternfly-react'

import { fetchAnomaliesAction } from '../reducers'


class BuildTooltip extends React.Component {
  static propTypes = {
    build: PropTypes.object.isRequired
  }

  render () {
    const { build } = this.props
    return (
      <div>
        Pipeline: {build.pipeline}<br />
        Project: {build.project}<br />
        Ref: {build.ref}
      </div>
    )
  }
}

class AnomalyList extends React.Component {
  static propTypes = {
    anomalies: PropTypes.array,
    dispatch: PropTypes.func
  }

  constructor () {
    super()

    this.prepareTableHeaders()
  }

  prepareTableHeaders () {
    const headerFormat = value => <Table.Heading>{value}</Table.Heading>
    const cellFormat = (value) => <Table.Cell>{value}</Table.Cell>
    const cellLinkFormat = (value) => (
      <Table.Cell>
        <Link to={'/view/' + value.uuid}>{value.name}</Link>
      </Table.Cell>
    )

    const cellBuildFormat = (value) => (
      <OverlayTrigger
        overlay={<Tooltip id="build"><BuildTooltip build={value} /></Tooltip>}
        placement="bottom"
        id="42"
        >
        <Table.Cell>
          {value.uuid.slice(0, 6)}
        </Table.Cell>
      </OverlayTrigger>
    )
    this.columns = []
    const myColumns = [
      'name',
      'build',
      'status',
      'reporter',
      'report_date'
    ]
    myColumns.forEach(column => {
      let prop = column
      let formatter = cellFormat
      if (prop === 'build') {
        formatter = cellBuildFormat
      } else if (prop === 'name') {
        formatter = cellLinkFormat
        prop = 'nameLink'
      }
      const label = column.charAt(0).toUpperCase() + column.slice(1)
      this.columns.push({
        header: {label: label, formatters: [headerFormat]},
        property: prop,
        cell: {formatters: [formatter]}
      })
    })
  }

  componentDidMount () {
    this.refresh()
  }

  refresh = () => {
    this.props.dispatch(fetchAnomaliesAction())
  }

  renderTable (anomalies) {
    anomalies.map(item => {
      if (!item.nameLink) {
        item.nameLink = {
          name: item.name,
          uuid: item.uuid,
        }
      }
      return item
    })
    return (
      <Table.PfProvider
        striped
        bordered
        hover
        columns={this.columns}
      >
        <Table.Header/>
        <Table.Body
          rows={anomalies}
          rowKey="uuid"
          />
      </Table.PfProvider>)
  }

  render () {
    const { anomalies } = this.props
    return (
      <React.Fragment>
        <a className="refresh pull-right" onClick={this.refresh}>
          <Icon type="fa" name="refresh" /> refresh
        </a>
        {anomalies ? this.renderTable(anomalies) : <p>Loading</p>}
      </React.Fragment>
    )
  }
}

export default connect(
  state => ({
    anomalies: state.anomalies
  })
)(AnomalyList)
